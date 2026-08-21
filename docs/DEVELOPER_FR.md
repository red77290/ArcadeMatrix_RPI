🇬🇧 [English](DEVELOPER.md) | 🇫🇷 Français | 🇪🇸 [Español](DEVELOPER_ES.md)

# Guide développeur (Raspberry Pi - Rust)

Bienvenue dans le guide de développement d'ArcadeMatrix. Ce document explique la marche à suivre pour étendre l'architecture et créer de nouveaux moteurs (Engines).

---

## 1. Comprendre l'Architecture : Engines, Registry et Lifecycle

ArcadeMatrix (RPi) ne possède plus de liste codée en dur de ses fonctionnalités. Le système repose sur un **Registry** asynchrone qui découvre les moteurs à la compilation (grâce à la macro `linkme::distributed_slice`).

### 1.1 Le Cycle de Vie Strict (Lazy-Once)

Pour maintenir des performances irréprochables (60 FPS stables, sans *jitter*), ArcadeMatrix impose un cycle de vie strict pour chaque `Engine`.

```text
initialize()
    │
    ├── allocation
    ├── chargement assets (images, polices)
    ├── préparation cache
    └── initialisation lourde
          ↓
activate()
    │
    └── préparation d'état temporaire (réinitialisation chrono, position de balle...)
          ↓
update()
    │
    └── logique temps réel (60 FPS) - **AUCUNE ALLOCATION DYNAMIQUE INUTILE**
          ↓
render()
    │
    └── rendu temps réel (60 FPS) - **AUCUNE ALLOCATION DYNAMIQUE INUTILE**
          ↓
deactivate()
    │
    └── libération de ressources externes ou arrêt des écouteurs
```

- **Règle d'or :** Ne créez jamais de nouveaux `String`, `Vec` ou structures lourdes dans `update()` ou `render()`. Pré-allouez vos tampons dans `initialize()` et mutez-les en place.
- **`on_config_changed()` :** Permet au moteur de mettre à jour son état interne lorsque l'utilisateur change les réglages sans repasser par `initialize()`.
- **`is_finished()` :** Utile pour signaler au gestionnaire de rotation qu'un moteur a terminé son scénario (ex: toutes les cryptos ont défilé) pour forcer le passage immédiat au moteur suivant.

---

## 2. Tutoriel : Créer un Nouveau Moteur (Engine)

Pour créer un nouveau moteur (ex: `SpotifyEngine`), vous devez implémenter le trait `Engine` et fournir un `EngineDescriptor`.

### Étape 1 : Créer le fichier

Créez `src/engines/spotify.rs` :

```rust
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError, EngineMetadata,
};
use crate::core::registry::ENGINES;
use linkme::distributed_slice;

pub struct SpotifyEngine {
    client_id: String,
}

impl SpotifyEngine {
    pub fn new() -> Self {
        Self {
            client_id: String::new(),
        }
    }
}
```

### Étape 2 : Implémenter le cycle de vie (Le Trait Engine)

Implémentez le comportement de votre moteur, en respectant la contrainte d'allocation.

```rust
impl Engine for SpotifyEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        // Chargement lourd, allocation de buffers, lecture des configs
        self.client_id = config.get_string("client_id", "");
        println!("SpotifyEngine initialisé !");
        Ok(())
    }

    fn activate(&mut self) {
        // Préparation au retour à l'écran
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Logique métier rapide, sans création de variables dynamiques
    }

    fn render(&mut self, _context: &mut EngineContext) {
        // Rendu matériel via _context.canvas
    }

    fn deactivate(&mut self) {
        // Le moteur n'est plus à l'écran
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        // Si l'utilisateur modifie config.json à chaud
        self.client_id = config.get_string("client_id", "");
    }

    fn is_finished(&self) -> bool {
        false // Renvoie true si la séquence de votre module est terminée
    }
}
```

### Étape 3 : Exposer le Descripteur (Registry)

Déclarez les champs de configuration nécessaires (qui apparaîtront automatiquement dans la Web UI) et injectez la Factory dans le registre de compilation :

```rust
#[distributed_slice(ENGINES)]
fn register_spotify_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "spotify",
            name: "Lecteur Spotify",
            category: "media",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: crate::core::engine_contract::Requirements::default(),
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "client_id",
                    field_type: ConfigType::String,
                    label: "Client ID",
                    description: "Votre clé API Spotify",
                    default_value: "",
                    options: None,
                    min_val: None,
                    max_val: None,
                    required: true,
                    step: None,
                    visible_when: None,
                },
            ],
        },
        factory: || Box::new(SpotifyEngine::new()),
    }
}
```

### Étape 4 : Déclarer le module

Ouvrez `src/engines/mod.rs` et ajoutez votre fichier pour que le compilateur l'intègre :

```rust
pub mod spotify;
```

C'est tout ! **Aucun code du Core (app.rs, registry.rs) n'a été modifié**. Le moteur sera automatiquement listé dans l'API Web et sa configuration sera gérée de manière isolée via le `ConfigSchema`.

---

## 3. ConfigSchema et Typage Dynamique

L'API Web n'a plus besoin d'être mise à jour manuellement lorsque vous ajoutez des champs de configuration.
Le type `ConfigType` supporte `String`, `Number`, `Boolean`, `Select`, et `Color`.
Les `ConfigField` seront renvoyés au format JSON au tableau de bord, qui générera les champs correspondants. Le Core stocke ces paires clé/valeur dans `config.json` et vous les redonne via `config.get_string`, `config.get_number`, etc.
