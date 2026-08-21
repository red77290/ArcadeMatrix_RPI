🇬🇧 [English](DEVELOPER.md) | 🇫🇷 Français | 🇪🇸 [Español](DEVELOPER_ES.md)

# Guide développeur (Raspberry Pi - Rust)

Bienvenue dans le guide de développement d'ArcadeMatrix pour Raspberry Pi. Ce document explique comment étendre l'architecture et créer de nouveaux Engines en Rust.

---

## 1. Comprendre l'architecture : Engines, Registry et cycle de vie

ArcadeMatrix ne possède plus de liste de fonctionnalités codée en dur dans `app.rs`. Le système repose sur un **Registry** (via la crate `linkme`) qui découvre les moteurs au démarrage.

### 1.1 Le cycle de vie strict (Lazy-Once)

Pour éviter le tearing et le jitter causés par l'allocateur mémoire de Rust (Heap), ArcadeMatrix impose un cycle de vie strict à chaque implémentation du trait `Engine`.

```text
initialize()
    │
    ├── heap allocations via 'String' or 'Vec'
    ├── loading assets (images, fonts)
    ├── caching setup
    └── heavy initialization
          ↓
activate()
    │
    └── temporary state preparation (resetting timers, etc.)
          ↓
update()
    │
    └── real-time logic (60 FPS) - **NO UNNECESSARY DYNAMIC ALLOCATIONS**
          ↓
render()
    │
    └── real-time rendering (60 FPS) - **NO UNNECESSARY DYNAMIC ALLOCATIONS**
          ↓
deactivate()
    │
    └── freeing external resources or stopping listeners
```

- **Règle d'or :** N'instanciez jamais de nouveaux `String` ou `Vec` dynamiques dans `update()` ou `render()`. Pré-allouez vos tampons dans `initialize()` et modifiez-les en place (ex. `my_string.clear()` puis `write!(&mut my_string, "...")`).
- **`on_config_changed()` :** Appelée **à chaud** par l'`EngineRuntime` chaque fois que la configuration persistée d'une instance mise en cache change (par exemple quand l'utilisateur la modifie dans la Web UI). Le moteur n'est **pas** recréé : il conserve ses allocations et relit simplement les nouvelles valeurs. Implémentez cette méthode pour appliquer les réglages sans redémarrage.
- **`is_finished()` :** Utile pour signaler à l'`EngineRuntime` qu'un moteur a terminé sa tâche afin de passer au moteur suivant sans attendre le timeout.

### 1.2 Capabilities & cadence de rafraîchissement

Le runtime déduit sa pause entre frames depuis les `Capabilities` du descripteur du moteur, **pas** depuis un nom de moteur codé en dur :

- `realtime: true` → le moteur est interrogé à ~25 FPS (40 ms) pour une animation fluide (GIF, message défilant, Spotify).
- `realtime: false` (défaut) → le moteur se rafraîchit une fois par seconde (1000 ms), idéal pour les contenus statiques (horloge, date, météo) et beaucoup plus léger pour le CPU/Wi-Fi.

Définissez `realtime: true` dans votre descripteur uniquement si votre moteur anime chaque frame.

### 1.3 Configuration autoréparatrice

Chaque valeur déclarée dans le `ConfigSchema` est validée par le `ConfigSanitizer` au démarrage et à chaque écriture. Pour en bénéficier, renseignez les métadonnées de champ pertinentes :

- `field_type` (`Integer`, `Float`, `Boolean`, `Options`, `String`) sélectionne la stratégie de validation.
- `min_val` / `max_val` bornent les champs numériques ; `options` liste les valeurs autorisées pour `Options`.
- `validation_policy` (`Clamp`, `FallbackDefault`, `Reject`, `Accept`) décide quoi faire d'une valeur hors limites.
- `default_value` est injecté automatiquement lorsque la clé est absente (ex. un champ ajouté par une OTA ultérieure). Les clés qui ne sont plus présentes dans le schéma sont supprimées.

---

## 2. Tutoriel : créer un nouvel Engine

Pour créer un nouveau moteur, vous devez implémenter le trait `Engine` et fournir un `EngineDescriptor` via le Registry.

### Étape 1 : créer la structure (`src/engines/my_engine.rs`)

```rust
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::matrix::MatrixBackend;

pub struct MyEngine {
    my_setting: String,
    counter: u32,
}

impl MyEngine {
    pub fn new() -> Self {
        Self {
            my_setting: String::new(),
            counter: 0,
        }
    }
}
```

### Étape 2 : implémenter le cycle de vie

```rust
impl Engine for MyEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        // Safe place for allocations
        self.my_setting = config.get_string("my_setting", "default");
        println!("MyEngine initialized!");
        Ok(())
    }

    fn activate(&mut self) {
        self.counter = 0; // Quick reset
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Fast business logic, NO allocations
        self.counter += 1;
    }

    fn render(&mut self, context: &mut EngineContext) {
        // Hardware rendering via context.matrix
        context.matrix.clear();
        // Caution: drawing text creates no allocation if using existing buffers
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.my_setting = config.get_string("my_setting", "default");
    }

    fn is_finished(&self) -> bool {
        false
    }
}
```

### Étape 3 : enregistrer l'Engine au démarrage

Ajoutez le descripteur en bas de votre fichier afin d'exposer les champs de configuration à l'API Web :

```rust
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, EngineDescriptor, EngineFactory,
    EngineMetadata, Requirements,
};
use linkme::distributed_slice;

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_MyEngine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "my_engine",
            name: "My Custom Engine",
            category: "misc",
            version: "1.0",
        },
        capabilities: Capabilities::default(), // set `realtime: true` if you animate every frame
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![ConfigField {
                id: "my_setting",
                field_type: ConfigType::String,
                label: "My Setting",
                description: "Enter a word to display",
                default_value: "default",
                options: None,
                min_val: None,
                max_val: None,
                required: false,
                step: None,
                visible_when: None,
                options_endpoint: None,
                multiple: false,
                // Drives the self-healing sanitizer for numeric/option fields.
                validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
            }],
        },
        factory: || Box::new(MyEngine::new()),
    }
}
```

### Étape 4 : ajouter la référence du module

Ouvrez `src/engines/mod.rs` et ajoutez :
```rust
pub mod my_engine;
```

C'est tout ! **Aucun code de `app.rs` n'a besoin d'être modifié**. Le moteur sera automatiquement listé dans l'API Web et sa configuration `config.json` sera gérée de manière isolée.
