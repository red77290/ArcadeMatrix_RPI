🇫🇷 Français | 🇬🇧 [English](ARCHITECTURE.md) | 🇪🇸 [Español](ARCHITECTURE_ES.md)

# Vue d'ensemble de l'architecture (Raspberry Pi - Rust)

Ce document est la référence **approfondie et exhaustive** de l'architecture ArcadeMatrix sur Raspberry Pi (écrite en **Rust**). Il couvre la philosophie de conception, le contrat complet des moteurs, le Registry d'auto-découverte, le cycle de vie « Lazy-Once », le pipeline de configuration auto-réparante, l'UI dynamique pilotée par schéma (y compris les **listes d'options personnalisées / dynamiques**), l'arbitre d'affichage, le compositeur d'overlay Fighter et le runtime multi-thread.

> Pour **ajouter** un moteur ou un champ de configuration, lisez [DEVELOPER_FR.md](DEVELOPER_FR.md). Ce document explique **pourquoi** et **comment** le système se comporte ; le guide développeur explique **quoi écrire**.

---

## Table des matières

1. [Philosophie : performance et « jitter »](#1-philosophie--performance-et--jitter)
2. [Carte des composants](#2-carte-des-composants)
3. [Le contrat des moteurs (modèle de classes)](#3-le-contrat-des-moteurs-modèle-de-classes)
4. [Auto-découverte : Registry, Descriptor et Factory](#4-auto-découverte--registry-descriptor-et-factory)
5. [Le cycle de vie « Lazy-Once »](#5-le-cycle-de-vie--lazy-once-)
6. [Modèle de configuration : `config.json` → instances](#6-modèle-de-configuration--configjson--instances)
7. [Auto-réparation : le ConfigSanitizer](#7-auto-réparation--le-configsanitizer)
8. [Propagation de config et hot reload](#8-propagation-de-config-et-hot-reload)
9. [UI dynamique pilotée par schéma et listes personnalisées](#9-ui-dynamique-pilotée-par-schéma-et-listes-personnalisées)
10. [L'arbitre d'affichage](#10-larbitre-daffichage)
11. [Le compositeur d'overlay Fighter](#11-le-compositeur-doverlay-fighter)
12. [Isolation runtime et modèle de threads](#12-isolation-runtime-et-modèle-de-threads)
13. [Cadence de rendu](#13-cadence-de-rendu)
14. [Surface de l'API HTTP](#14-surface-de-lapi-http)
15. [Métadonnées de build](#15-métadonnées-de-build)

---

## 1. Philosophie : performance et « jitter »

Contrairement à l'ESP32, le Raspberry Pi dispose de RAM abondante (512 Mo à 8 Go). Cependant, son système d'exploitation n'est **pas** temps réel (pas de RTOS). Le pilote de la matrice (via DMA/GPIO, `rpi-rgb-led-matrix`) est extrêmement sensible aux micro-saccades (« jitter »).

Pour maintenir un rafraîchissement stable sans déchirure, **la boucle chaude (`update()` + `render()`) ne doit effectuer aucune allocation dynamique inutile**. Chaque allocation tas risque un `malloc`/redimensionnement qui introduit quelques millisecondes de latence imprévisible — suffisant pour faire scintiller le panneau.

Trois règles en découlent et façonnent toute l'architecture :

- **Allouer une fois, muter sur place.** Les buffers (`String`, `Vec`) sont réservés dans `initialize()` et réutilisés à chaque frame (`clear()` + `write!()`).
- **Créer les moteurs paresseusement, les garder à vie.** Un moteur n'est instancié qu'à son premier affichage, puis mis en cache pour toute la durée du processus (« Lazy-Once »).
- **Isoler le thread de rendu.** HTTP, MQTT et I/O réseau ne s'exécutent jamais sur le thread qui parle à la matrice.

---

## 2. Carte des composants

```mermaid
flowchart TD
    subgraph Boot
        MAIN["main.rs"] --> CFG["Config.load() + ConfigSanitizer"]
    end

    CFG --> REND["matrix-render thread (8MB stack)"]
    CFG --> API["api-server thread (actix, port 80)"]

    subgraph RenderThread["matrix-render thread"]
        REND --> ARB["DisplayArbiter.evaluate()"]
        ARB --> ROT["RotationState"]
        ROT --> RT["EngineRuntime (Lazy-Once cache)"]
        RT --> REG["EngineRegistry (auto-discovery)"]
        REG --> ENG["Box&lt;dyn Engine&gt;"]
        ENG --> MX["MatrixBackend"]
        RT --> OV["FighterEngine overlay pass"]
        OV --> MX
    end

    subgraph ApiThread["api-server thread"]
        API --> EP["REST endpoints /api/*"]
        EP --> REG
        EP --> SAN["ConfigSanitizer"]
        EP --> SAVE["config.json (atomic save)"]
    end

    API -. "AtomicBool: reload_flag / reset_rotation" .-> REND
    SAVE -. "RwLock&lt;ConfigSettings&gt;" .-> REND
```

Les deux threads **ne partagent jamais d'état mutable directement**. Ils communiquent uniquement via :

- un `Config` partagé protégé par `RwLock<ConfigSettings>` (pour le snapshot des réglages), et
- des atomiques sans verrou (`AtomicBool` / `AtomicU32`) utilisés comme signaux one-shot.

---

## 3. Le contrat des moteurs (modèle de classes)

Chaque fonctionnalité visuelle (horloge, météo, lecteur GIF, ticker crypto…) implémente l'unique trait `Engine`. Le Core ne manipule jamais qu'un `Box<dyn Engine>` — il n'a **aucune connaissance à la compilation** des types concrets.

```mermaid
classDiagram
    class Engine {
        <<trait>>
        +initialize(ctx, config) Result
        +activate()
        +update(ctx)
        +render(ctx)
        +deactivate()
        +on_config_changed(config)
        +is_finished() bool
        +is_realtime() bool
        +set_rotation_budget(budget)
        +self_paced() bool
    }

    class EngineDescriptor {
        +EngineMetadata metadata
        +Capabilities capabilities
        +Requirements requirements
        +ConfigSchema schema
        +EngineFactory factory
    }

    class EngineMetadata {
        +str id
        +str name
        +str category
        +str version
    }

    class Capabilities {
        +bool supports_128x32
        +bool supports_256x64
        +bool realtime
        +bool interruptible
    }

    class Requirements {
        +bool needs_audio
        +bool needs_network
        +bool needs_sd
    }

    class ConfigSchema {
        +Vec~ConfigField~ fields
    }

    class ConfigField {
        +str id
        +ConfigType field_type
        +str label
        +str description
        +str default_value
        +bool required
        +Option~str~ min_val
        +Option~str~ max_val
        +Option~str~ step
        +Option~Vec~ options
        +Option~str~ visible_when
        +Option~str~ options_endpoint
        +bool multiple
        +ValidationPolicy validation_policy
    }

    class EngineContext {
        +MatrixBackend matrix
        +Config config
    }

    class EngineConfig {
        <<trait>>
        +get_string(key, default) String
        +get_int(key, default) i32
        +get_bool(key, default) bool
    }

    EngineDescriptor --> EngineMetadata
    EngineDescriptor --> Capabilities
    EngineDescriptor --> Requirements
    EngineDescriptor --> ConfigSchema
    EngineDescriptor ..> Engine : factory builds
    ConfigSchema "1" --> "*" ConfigField
    ConfigField --> ConfigType
    ConfigField --> ValidationPolicy
    Engine ..> EngineContext : uses
    Engine ..> EngineConfig : reads
```

### Responsabilités des méthodes

| Méthode | Appelée | Rôle |
| :-- | :-- | :-- |
| `initialize` | une fois, au premier affichage | Allocation lourde : charger bitmaps/polices, réserver les buffers. |
| `activate` | à chaque passage à l'écran | Réinitialisation légère de l'état transitoire (sans allocation). |
| `update` | boucle chaude | Logique métier. **Aucune allocation inutile.** |
| `render` | boucle chaude | Dessin dans `context.matrix`. **Aucune allocation inutile.** |
| `deactivate` | en quittant l'écran | Arrêter les tâches/écouteurs de fond. |
| `on_config_changed` | à l'édition live | Relire les valeurs **sur place**, sans recréation. |
| `is_finished` | chaque frame | Signaler au runtime d'avancer plus tôt (ex. le crypto a fini sa liste de jetons). |
| `is_realtime` | chaque frame | Indice de cadence live (≈25 FPS) évalué par frame, contrairement au `Capabilities.realtime` statique. |
| `set_rotation_budget` | à l'activation | Pour les moteurs basés sur un compteur (GIF), reçoit la valeur numérique de l'entrée de rotation comme budget de lecture. |
| `self_paced` | chaque frame | Si `true`, le minuteur de durée ne doit **pas** forcer l'avance ; le moteur pilote l'avance via `is_finished`. |

---

## 4. Auto-découverte : Registry, Descriptor et Factory

### Pourquoi le Core n'a aucune liste de types concrets

Dans les versions d'avant refonte, `app.rs` incluait chaque fichier moteur et construisait un énorme `match` avec `Box::new(ClockEngine)`. Ajouter un moteur imposait de modifier le Core — une violation du principe ouvert/fermé (SOLID).

Désormais chaque moteur **s'enregistre à la compilation** via le `#[distributed_slice]` de la crate `linkme`. Le linker collecte chaque fonction d'enregistrement dans une slice statique unique `ENGINES` ; le Core l'itère simplement.

```rust
// core/registry.rs
#[distributed_slice]
pub static ENGINES: [fn() -> EngineDescriptor];
```

```rust
// n'importe quel fichier moteur
#[distributed_slice(crate::core::registry::ENGINES)]
fn register_clock() -> EngineDescriptor { /* metadata + schema + factory */ }
```

### Pourquoi le Registry stocke des descripteurs, pas des instances

Instancier chaque moteur au boot (`Box::new(...)`) gaspillerait de la RAM et ralentirait le démarrage. Un **descripteur** est léger : il porte les métadonnées, capacités, prérequis, le schéma de configuration, et une **factory** — un pointeur de fonction `fn() -> Box<dyn Engine>` qui construit l'instance uniquement au besoin.

```mermaid
flowchart LR
    ID["engine_id (e.g. 'clock')"] --> REG["EngineRegistry.get_descriptor(id)"]
    REG --> DESC["EngineDescriptor"]
    DESC --> FAC["factory()"]
    FAC --> INST["Box&lt;dyn Engine&gt;"]
```

`EngineRegistry` expose deux appels :

- `get_all_descriptors()` — utilisé par `GET /api/engines` et le sanitizer.
- `get_descriptor(id)` — utilisé par le runtime pour construire une instance.

---

## 5. Le cycle de vie « Lazy-Once »

L'`EngineRuntime` possède deux maps : les instances vivantes en cache et un snapshot de la config avec laquelle chacune a été configurée en dernier.

```rust
pub struct EngineRuntime {
    instances: HashMap<String, Box<dyn Engine>>,     // instance_id -> moteur vivant
    configs:   HashMap<String, HashMap<String,String>>, // instance_id -> dernière config appliquée
}
```

`get_instance()` est le cœur du Lazy-Once et du hot-reload :

```mermaid
sequenceDiagram
    participant Loop as Render loop
    participant RT as EngineRuntime
    participant Reg as EngineRegistry
    participant Eng as Engine

    Loop->>RT: get_instance(instance_id, engine_id, ctx, config_map)
    alt instance not cached
        RT->>Reg: get_descriptor(engine_id)
        Reg-->>RT: EngineDescriptor
        RT->>Eng: factory()
        RT->>Eng: initialize(ctx, config)
        RT->>RT: cache instance + config snapshot
    else instance already alive
        RT->>RT: config_map != last snapshot ?
        alt config changed
            RT->>Eng: on_config_changed(config)
            RT->>RT: update snapshot
        end
    end
    RT-->>Loop: &mut Box(dyn Engine)
```

Le cycle de vie sous forme de machine à états :

```mermaid
stateDiagram-v2
    [*] --> Uninstantiated
    Uninstantiated --> Initialized : first display / factory + initialize()
    Initialized --> Active : activate()
    Active --> Active : update() + render() (hot loop)
    Active --> Active : on_config_changed() (live edit)
    Active --> Standby : deactivate()
    Standby --> Active : activate()
    Active --> [*] : is_finished() advances rotation
```

**Propriété clé :** une édition de configuration ne détruit ni ne reconstruit jamais une instance. L'instance garde ses buffers et relit simplement les valeurs dans `on_config_changed()`.

---

## 6. Modèle de configuration : `config.json` → instances

L'unique fichier racine `config.json` décrit tout l'appareil. Sa structure :

```mermaid
classDiagram
    class ConfigSettings {
        +MatrixConfig matrix
        +WifiConfig wifi
        +MqttConfig mqtt
        +SystemConfig system
        +Vec~EngineInstance~ instances
        +Vec~RotationEntry~ rotation
        +bool api_auth_enabled
        +str api_token
    }
    class EngineInstance {
        +str instance_id
        +str engine_id
        +HashMap~String,String~ config
    }
    class RotationEntry {
        +str instance_id
        +u32 duration_sec
        +bool fighter_overlay
    }
    class SystemConfig {
        +str timezone
        +bool format_24h
        +u32 day_brightness
        +u32 night_brightness
        +bool idle_fighter_enabled
        +u32 idle_fighter_interval
        +...
    }
    ConfigSettings "1" --> "*" EngineInstance
    ConfigSettings "1" --> "*" RotationEntry
    ConfigSettings --> MatrixConfig
    ConfigSettings --> SystemConfig
    ConfigSettings --> WifiConfig
    ConfigSettings --> MqttConfig
```

### Trois concepts distincts

- **Moteur (Engine)** — un *type* (ex. `clock`), déclaré une fois par le Registry.
- **Instance** — une *occurrence nommée et configurée* d'un moteur (ex. `clock_main`, `clock_arcade`), stockée dans `instances`.
- **Configuration** — le `HashMap<String,String>` d'une instance, validé contre le `ConfigSchema` du moteur.

C'est pourquoi vous pouvez faire tourner plusieurs horloges avec des polices/thèmes différents à partir du même `ClockEngine`.

### Pourquoi `config.json` et `EngineConfig` sont séparés

Les moteurs ne doivent pas voir les identifiants WiFi ni les réglages des autres moteurs. Le runtime enveloppe le `HashMap` de chaque instance dans un `HashConfig` et ne remet au moteur que le trait `EngineConfig` (`get_string/get_int/get_bool`) — un proxy restreint exposant exactement les clés déclarées par le moteur dans son schéma.

### Les signaux runtime vivent sur `Config`

`Config` porte aussi l'état runtime inter-threads, distinct du `ConfigSettings` persistant :

```rust
pub struct Config {
    pub reload_flag: AtomicBool,      // changement matériel/réseau -> redémarrage propre
    pub reset_rotation: AtomicBool,   // édition instance/rotation -> relecture à la frame suivante
    pub matrix_power: AtomicBool,     // on/off live
    pub matrix_brightness: AtomicU32, // luminosité live (0..100)
    pub message_payload: Mutex<Option<Value>>,
    pub settings: RwLock<ConfigSettings>,
}
```

---

## 7. Auto-réparation : le ConfigSanitizer

`ConfigSanitizer::sanitize_instances()` s'exécute au boot et après chaque écriture. Pour chaque instance, il retrouve le schéma du moteur et répare la config stockée afin que le runtime voie toujours des données valides — c'est ce qui rend les mises à jour OTA robustes.

```mermaid
flowchart TD
    START["for each instance"] --> SCHEMA{engine_id in Registry?}
    SCHEMA -- no --> INVALID["count invalid_instance, skip"]
    SCHEMA -- yes --> FIELD["for each schema field"]
    FIELD --> PRESENT{key present?}
    PRESENT -- no --> INJECT["inject default_value"]
    PRESENT -- yes --> TYPE{field_type}
    TYPE -- Integer/Float --> RANGE{in min..max?}
    RANGE -- no --> POLICY{validation_policy}
    POLICY -- Clamp --> CLAMP["clamp to bound"]
    POLICY -- FallbackDefault --> FB1["reset to default"]
    POLICY -- Reject/Accept --> KEEP1["leave as-is"]
    TYPE -- Boolean --> NORM["normalize true/1/yes/on -> true"]
    TYPE -- Options --> OPT{value in allowed?}
    OPT -- no --> FB2["reset to default"]
    OPT -- "dynamic (options_endpoint)" --> KEEP2["leave as-is"]
    TYPE -- String --> KEEP3["accept"]
    FIELD --> PRUNE["prune keys not in schema"]
```

`SanitizeResult` indique combien de valeurs ont été `defaults_injected`, `values_clamped`, `values_fallback`, `keys_pruned` et `invalid_instances`, ainsi que si le fichier a été `modified` (déclenchant une ré-sauvegarde).

Deux subtilités importantes :

- **Les options dynamiques sont de confiance.** Un champ avec `options_endpoint` (ex. un nom de fichier de police) n'a pas de liste blanche statique à la compilation ; le sanitizer laisse donc sa valeur intacte.
- **Le multi-sélection est un CSV.** Quand `multiple = true`, la valeur est une liste séparée par des virgules ; chaque jeton doit appartenir à l'ensemble autorisé.

Exemple OTA concret — le firmware v2 ajoute `font_size` et retire `legacy_mode` :

```jsonc
// stocké (v1)                 // après boot en v2
{ "font": "foo" }        -->   { "font": "foo", "font_size": "16" }
{ "legacy_mode": "x" }   -->   {}   // élagué : plus dans le schéma
```

---

## 8. Propagation de config et hot reload

Comme les instances sont en cache, une édition doit être **poussée activement** vers le moteur vivant plutôt que de le recréer. La chaîne est câblée de bout en bout :

```mermaid
sequenceDiagram
    participant UI as Web UI
    participant API as api-server thread
    participant Disk as config.json
    participant Flag as reset_rotation (AtomicBool)
    participant Loop as matrix-render loop
    participant RT as EngineRuntime
    participant Eng as Engine

    UI->>API: POST /api/instances {id, engine_id, config}
    API->>API: validate engine_id
    API->>API: ConfigSanitizer.sanitize_instances()
    API->>Disk: atomic save
    API->>Flag: store(true)
    Note over Loop: next frame
    Loop->>Flag: swap(false)
    Loop->>RT: get_instance(... new config_map)
    RT->>RT: snapshot changed?
    RT->>Eng: on_config_changed(config)
    Eng-->>Loop: renders with new values (no realloc)
```

Deux classes de propagation :

- **Éditions d'instance / rotation** → `reset_rotation` → appliquées **en live** via `on_config_changed()` ; sans redémarrage ni réallocation.
- **Changements matériel / réseau** (géométrie matrice, `disable_internal`…) → `reload_flag` → la boucle de rendu redémarre proprement le processus pour réinitialiser le pilote. La luminosité/puissance live font exception : elles passent par les atomiques `matrix_brightness` / `matrix_power` sans redémarrage.

---

## 9. UI dynamique pilotée par schéma et listes personnalisées

L'UI web ne contient **aucun formulaire par moteur**. `GET /api/engines` renvoie chaque descripteur (métadonnées + schéma), et `dynamic_engines.js` interprète chaque `ConfigField` pour construire le bon widget. Ajouter un moteur ou un champ change l'UI sans aucune ligne de frontend.

### Résolution champ → widget

```mermaid
flowchart TD
    F["ConfigField"] --> OE{options_endpoint set?}
    OE -- yes --> M{multiple?}
    M -- yes --> CB["checkbox grid (CSV value)"]
    M -- no --> DD1["dropdown from endpoint"]
    OE -- no --> T{field_type}
    T -- Options --> DD2["dropdown from static options"]
    T -- Boolean --> SEL["Enabled/Disabled select"]
    T -- "id contains 'color'" --> COL["color picker"]
    T -- Integer/Float --> NUM["number input (min/max)"]
    T -- String --> TXT["text input"]
```

### Listes d'options personnalisées / dynamiques (les endpoints de « découverte de ressources »)

C'est le mécanisme que l'ancienne UI codée en dur perdait. Un champ déclare **d'où** viennent ses choix au lieu de les coder en dur ; le backend sert les ressources réelles et à jour :

| Endpoint | Source | Utilisé par (champ) |
| :-- | :-- | :-- |
| `GET /api/fonts` | fichiers dans `fonts/` (`.ttf`, `.bdf`) | `font` de l'horloge, tout moteur de texte |
| `GET /api/playlists` | sous-dossiers de `gifs/` | `playlist` du GIF (**multiple**) |
| `GET /api/themes` | `core::theme::all_themes()` (source unique de vérité) | `theme` de l'horloge |

Chacun renvoie un tableau JSON de `{ "value": ..., "label": ... }`. Comme la liste est récupérée **en live**, déposer une nouvelle police dans `fonts/` ou un nouveau dossier GIF dans `gifs/` apparaît immédiatement dans l'UI.

```mermaid
sequenceDiagram
    participant UI as dynamic_engines.js
    participant API as api-server
    participant FS as filesystem / theme table

    UI->>API: GET /api/engines
    API-->>UI: descriptors (schema incl. options_endpoint)
    loop each field with options_endpoint
        UI->>API: GET {options_endpoint}
        API->>FS: read fonts/ | gifs/ | themes
        FS-->>API: entries
        API-->>UI: [{value,label}, ...]
        UI->>UI: build dropdown or checkbox grid
    end
```

### Stockage du multi-sélection

Pour `multiple = true` (ex. la playlist GIF), l'UI affiche une grille de cases à cocher et stocke la sélection sous forme de **chaîne séparée par des virgules** dans la config d'instance (`"mario,zelda"`). Le moteur GIF et le sanitizer découpent tous deux sur `,`. C'est ainsi que l'utilisateur choisit *quels* dossiers GIF jouent — remplaçant l'ancien cas particulier « ignorer ceci, inclure cela » par un choix explicite et déclaratif.

### `visible_when`

Un champ peut porter `visible_when` référençant un autre champ, permettant au frontend de l'afficher/masquer conditionnellement (champs dépendants déclaratifs) sans JS spécifique au moteur.

---

## 10. L'arbitre d'affichage

La rotation n'est pas la seule à pouvoir posséder l'écran. Les marquees (frontends d'arcade), bannières MQTT, messages one-shot et le lecteur GIF se disputent l'affichage. Le `DisplayArbiter` tranche par **priorité**, si bien que le Core ne contient jamais de logique métier `if source == "mqtt"` dans la boucle de rendu.

```mermaid
classDiagram
    class DisplayArbiter {
        +Vec~DisplayRequest~ requests
        +submit_request(req)
        +cancel_request(source)
        +clear_expired()
        +evaluate() Option~DisplayRequest~
    }
    class DisplayRequest {
        +str source
        +DisplayPriority priority
        +RequestLifecycle lifecycle
        +bool preemptive
        +str instance_id
        +Option~Duration~ timeout
        +Instant created_at
    }
    class DisplayPriority {
        <<enum>>
        Rotation = 10
        Gif = 20
        Marquee = 30
        Visualizer = 40
        Mqtt = 100
    }
    class RequestLifecycle {
        <<enum>>
        OneShot
        Timed
        UntilCancelled
        Persistent
    }
    DisplayArbiter "1" --> "*" DisplayRequest
    DisplayRequest --> DisplayPriority
    DisplayRequest --> RequestLifecycle
```

À chaque frame, la boucle de rendu soumet/annule des requêtes selon l'état live, puis appelle `evaluate()`, qui écarte les requêtes expirées et renvoie la survivante de **plus haute priorité**. `ROTATION` est une base `Persistent`, non préemptive (priorité 10) toujours présente ; tout le reste (MQTT=100, Marquee=30, GIF=20…) peut prendre temporairement la main.

```mermaid
flowchart LR
    subgraph Frame
        S["submit/cancel requests"] --> E["evaluate()"]
        E --> CE["clear_expired()"]
        CE --> MAX["max_by priority"]
    end
    MAX --> WIN["winning source renders this frame"]
```

---

## 11. Le compositeur d'overlay Fighter

Le Fighter n'est **pas** un `Engine` et n'est **pas** arbitré. C'est un *overlay additif* : des sprites de combattants décoratifs dessinés **par-dessus** la frame de rotation courante. Comme l'arbitre est exclusif (un seul gagnant par frame), un overlay ne peut pas être modélisé comme une source concurrente — c'est donc une passe de composition séparée.

```mermaid
sequenceDiagram
    participant Loop as Render loop
    participant Eng as Active engine
    participant MX as MatrixBackend
    participant FE as FighterEngine

    Loop->>Eng: update() + render(ctx)
    Note over Loop: EngineContext scope closes (matrix borrow freed)
    Loop->>Loop: gate = idle_fighter_enabled AND entry.fighter_overlay
    alt overlay on
        Loop->>FE: set_interval(idle_fighter_interval)
        Loop->>FE: composite(matrix)
        FE-->>Loop: is_active() -> keep realtime cadence
    else overlay off
        Loop->>FE: stop() if active
    end
    Loop->>MX: update()
```

Décisions de conception :

- **Opt-in par entrée.** Chaque `RotationEntry` possède `fighter_overlay: bool`. L'overlay s'affiche seulement quand le **maître** (`system.idle_fighter_enabled`) *et* le drapeau de l'entrée courante sont tous deux vrais. Il n'y a délibérément **aucune** capacité « masquer sur GIF » automatique — l'utilisateur décide par écran.
- **Cycle de vie autogéré.** `FighterEngine` charge les sprites dans un thread de fond, planifie les combats sur son propre intervalle, et choisit le jeu d'assets selon la hauteur du panneau (`fighters_64` si hauteur ≥ 64, sinon `fighters_32`, avec repli en dernier recours sur l'autre jeu).
- **Couplage de cadence.** Tant qu'un combat est à l'écran, la boucle garde le chemin haute cadence pour que l'animation reste fluide, même par-dessus une horloge statique.

---

## 12. Isolation runtime et modèle de threads

```mermaid
flowchart TD
    subgraph P["Process"]
        direction TB
        R["matrix-render thread<br/>8MB stack, exclusive matrix access"]
        A["api-server thread<br/>single-threaded Tokio"]
        B["background: MQTT listener + HTTP API pollers"]
    end
    A -. atomics / RwLock .-> R
    B -. channels / mutex .-> R
    R --> HW["LED matrix (DMA/GPIO)"]
```

1. **Thread de rendu dédié (`matrix-render`)** — pile de 8 Mo, propriété exclusive de la matrice. S'il partageait le thread avec HTTP, chaque requête sauterait une frame (déchirure).
2. **Thread Web API isolé (`api-server`)** — un runtime Tokio mono-thread hébergeant actix sur le port 80. Il ne touche au thread de rendu que via des atomiques et de courtes lectures `RwLock`.
3. **Services de fond** — écouteur MQTT et pollers HTTP (crypto, météo, bourse) tournent hors du chemin de rendu, pour qu'un appel réseau lent ne bloque jamais `update()`.

---

## 13. Cadence de rendu

La pause par frame est dérivée de la capacité/état, **jamais** d'un nom de moteur codé en dur :

- `Capabilities.realtime == true` **ou** `engine.is_realtime() == true` en live → ~25 FPS (40 ms), pour le contenu animé (GIF, message défilant, Spotify, overlay Fighter actif).
- sinon → 1 Hz (1000 ms), pour le contenu statique (horloge, date, météo) — bien plus léger pour le CPU et le Wi-Fi.

`is_realtime()` est réévalué chaque frame, donc un moteur peut changer de cadence selon son état live (ex. une horloge qui n'anime que sur un thème précis).

---

## 14. Surface de l'API HTTP

Tous les endpoints sont des handlers actix dans `src/api/server.rs` ; les assets web statiques sont embarqués via `rust-embed`. Référence complète dans [../openapi.yaml](../openapi.yaml).

| Méthode | Chemin | Rôle |
| :-- | :-- | :-- |
| GET | `/api/system` | Snapshot complet des réglages |
| POST | `/api/system` | Patch des réglages top-level/système (sauvegarde partielle sûre) |
| GET | `/api/instances` | Lister les instances configurées |
| POST | `/api/instances` | Upsert d'une instance (assainie + sauvegardée) |
| DELETE | `/api/instances/{id}` | Supprimer une instance |
| GET | `/api/rotation` | Liste de rotation (ordre, durées, drapeaux overlay) |
| POST | `/api/rotation` | Remplacer la rotation, pose `reset_rotation` |
| GET | `/api/engines` | Tous les descripteurs (pilote l'UI dynamique) |
| GET | `/api/fonts` | Fichiers de polices de `fonts/` (options_endpoint) |
| GET | `/api/playlists` | Dossiers GIF de `gifs/` (options_endpoint) |
| GET | `/api/themes` | Thèmes de `core::theme` (options_endpoint) |
| GET | `/api/stats` | Stats runtime (uptime, mémoire, version) |
| POST | `/api/wifi` | Mettre à jour les identifiants Wi-Fi |
| POST | `/api/marquee` | Pousser une image de marquee |
| POST | `/api/mqtt/install` | Installer/activer le broker MQTT |
| POST | `/api/mqtt/logs` | Récupérer les logs MQTT |
| POST | `/api/system/restart` | Redémarrer le service |
| GET | `/api/action/reboot` · POST `/api/system/reboot` | Redémarrer le Pi |
| POST | `/api/system/shutdown` | Éteindre le Pi |
| POST | `/api/system/power` | Marche/arrêt live de la matrice |

Chaque handler mutateur passe derrière `check_auth` quand `api_auth_enabled` est actif.

---

## 15. Métadonnées de build

`core/build_info.rs` centralise les valeurs `env!` injectées par `build.rs` (`VERSION`, `ARCH`, `BUILD_TIMESTAMP`, `GIT_COMMIT`). Elles sont lues **une seule fois** ici car `env!` fige les valeurs à la compilation de chaque site d'appel ; les lire dans un module unique garde `/api/version`, la bannière de démarrage et la validation OTA cohérents entre builds incrémentaux.
