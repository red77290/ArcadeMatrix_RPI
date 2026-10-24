//! Retro Games Showcase Engine (`retro_games`).
//!
//! Displays localized boxart/flyer mini-jacket, complete scrolling synopsis,
//! granular top-to-bottom panoramas (`jacket_panorama`, `marquee_panorama`,
//! `snapshot_panorama_count`), and gameplay video snaps prepared live by
//! `dmd-media-showcase` across 28 arcade/console systems and 16 game genres.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::engines::renderers::media_showcase::{
    build_common_showcase_fields, MediaShowcaseRunner, ShowcaseDomain,
};
use linkme::distributed_slice;

pub struct RetroGamesEngine {
    runner: MediaShowcaseRunner,
}

impl RetroGamesEngine {
    pub fn new() -> Self {
        Self {
            runner: MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32),
        }
    }
}

impl Default for RetroGamesEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for RetroGamesEngine {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.runner.initialize(context, config);
        Ok(())
    }

    fn activate(&mut self) {
        self.runner.activate();
    }

    fn update(&mut self, context: &mut EngineContext) {
        self.runner.update(context);
    }

    fn render(&mut self, context: &mut EngineContext) {
        self.runner.render(context);
    }

    fn deactivate(&mut self) {
        self.runner.deactivate();
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.runner.on_config_changed(config);
    }

    fn has_new_frame(&self) -> bool {
        self.runner.dirty
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn self_paced(&self) -> bool {
        true
    }

    fn set_rotation_budget(&mut self, budget: u32) {
        self.runner.set_rotation_budget(budget);
    }

    fn is_finished(&self) -> bool {
        self.runner.is_finished()
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_retro_games_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("auto_action");
    fields.push(ConfigField {
        id: "systems",
        field_type: ConfigType::Options,
        label: "Arcade & Console Systems (Dynamic Referential)",
        description:
            "Select arcade, console, handheld, and home computer platforms fetched dynamically from dmd-media-showcase",
        default_value: "arcade,neogeo,cps1,cps2,snes,megadrive,psx,dreamcast",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/games/filters"),
        validation_policy: ValidationPolicy::Accept,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Game Genres (Dynamic Referential)",
        description:
            "Filter retro & arcade games by genre (relationally filtered by selected systems)",
        default_value: "action,fighting,shootemup,runandgun,platform,beatemup,rpg,racing",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/games/filters"),
        validation_policy: ValidationPolicy::Accept,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades (Dynamic Referential)",
        description: "Filter games by release era (relationally filtered by selected systems)",
        default_value: "1980s,1990s,2000s",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/games/filters"),
        validation_policy: ValidationPolicy::Accept,
        ..Default::default()
    });

    EngineDescriptor {
        metadata: EngineMetadata {
            id: "retro_games",
            name: "RetroGamesShowcase",
            category: "media",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            ..Default::default()
        },
        requirements: Requirements {
            needs_network: true,
            ..Default::default()
        },
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields },
        factory: || Box::new(RetroGamesEngine::new()),
    }
}
