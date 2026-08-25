use crate::core::engine_contract::{
    Capabilities, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext, EngineDescriptor,
    EngineError, EngineMetadata,
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

impl Engine for SpotifyEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.client_id = config.get_string("client_id", "");
        println!(
            "SpotifyEngine initialized with client_id: {}",
            self.client_id
        );
        Ok(())
    }

    fn activate(&mut self) {
        println!("SpotifyEngine activated");
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Dummy logic
    }

    fn render(&mut self, _context: &mut EngineContext) {
        // Dummy logic
    }

    fn deactivate(&mut self) {
        println!("SpotifyEngine deactivated");
    }

    fn is_finished(&self) -> bool {
        false
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.client_id = config.get_string("client_id", "");
    }
}

#[distributed_slice(ENGINES)]
fn register_spotify_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "spotify",
            name: "Spotify Player",
            category: "media",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            ..Default::default()
        },
        requirements: crate::core::engine_contract::Requirements::default(),
        schema: ConfigSchema {
            fields: vec![crate::core::engine_contract::ConfigField {
                id: "client_id",
                field_type: ConfigType::String,
                label: "Spotify Client ID",
                description: "Your Spotify API Client ID",
                default_value: "",
                options: None,
                min_val: None,
                max_val: None,
                required: true,
                ..Default::default()
            }],
        },
        factory: || Box::new(SpotifyEngine::new()),
    }
}
