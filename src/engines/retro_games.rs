//! Retro Games Showcase Engine (`retro_games`).
//!
//! Displays localized boxart/wheel, synopsis, pan-scan title screen, and gameplay
//! video snaps (ScreenScraper / Steam / Built-in Arcade Catalog) prepared by `dmd-media-showcase`.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
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

    fn is_finished(&self) -> bool {
        self.runner.is_finished()
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_retro_games_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("pixel_art");
    fields.push(ConfigField {
        id: "systems",
        field_type: ConfigType::Options,
        label: "Arcade & Console Systems",
        description: "Selected arcade and retro console systems to showcase",
        default_value: "mame,neogeo,snes,megadrive,psx",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "MAME Arcade",
                value: "mame",
            },
            ConfigOption {
                label: "SNK Neo-Geo",
                value: "neogeo",
            },
            ConfigOption {
                label: "Super Nintendo (SNES)",
                value: "snes",
            },
            ConfigOption {
                label: "Sega Mega Drive / Genesis",
                value: "megadrive",
            },
            ConfigOption {
                label: "Sony PlayStation (PSX)",
                value: "psx",
            },
            ConfigOption {
                label: "Nintendo NES",
                value: "nes",
            },
            ConfigOption {
                label: "Game Boy Advance (GBA)",
                value: "gba",
            },
            ConfigOption {
                label: "PC Engine / TurboGrafx-16",
                value: "pcengine",
            },
            ConfigOption {
                label: "Steam PC / Arcade Ports",
                value: "steam",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
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
