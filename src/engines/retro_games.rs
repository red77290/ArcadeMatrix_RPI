//! Retro Games Showcase Engine (`retro_games`).
//!
//! Displays localized boxart/flyer mini-jacket, complete scrolling synopsis,
//! granular top-to-bottom panoramas (`jacket_panorama`, `marquee_panorama`,
//! `snapshot_panorama_count`), and gameplay video snaps prepared live by
//! `dmd-media-showcase` across 28 arcade/console systems and 16 game genres.

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
        label: "Arcade & Console Systems (28 Platforms)",
        description:
            "Select arcade, console, handheld, and home computer platforms to discover live",
        default_value: "arcade,neogeo,cps1,cps2,snes,megadrive,psx,dreamcast",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Arcade / MAME Classics",
                value: "arcade",
            },
            ConfigOption {
                label: "SNK Neo-Geo MVS / AES",
                value: "neogeo",
            },
            ConfigOption {
                label: "Capcom CPS-1",
                value: "cps1",
            },
            ConfigOption {
                label: "Capcom CPS-2",
                value: "cps2",
            },
            ConfigOption {
                label: "Capcom CPS-3",
                value: "cps3",
            },
            ConfigOption {
                label: "FinalBurn Neo",
                value: "fbneo",
            },
            ConfigOption {
                label: "Sega Mega Drive / Genesis",
                value: "megadrive",
            },
            ConfigOption {
                label: "Super Nintendo (SNES)",
                value: "snes",
            },
            ConfigOption {
                label: "Nintendo Entertainment System (NES)",
                value: "nes",
            },
            ConfigOption {
                label: "Sega Master System",
                value: "mastersystem",
            },
            ConfigOption {
                label: "PC Engine / TurboGrafx-16",
                value: "pcengine",
            },
            ConfigOption {
                label: "Sony PlayStation (PS1)",
                value: "psx",
            },
            ConfigOption {
                label: "Sega Saturn",
                value: "saturn",
            },
            ConfigOption {
                label: "Sega Dreamcast",
                value: "dreamcast",
            },
            ConfigOption {
                label: "Nintendo 64 (N64)",
                value: "n64",
            },
            ConfigOption {
                label: "Nintendo Game Boy (GB)",
                value: "gb",
            },
            ConfigOption {
                label: "Game Boy Color (GBC)",
                value: "gbc",
            },
            ConfigOption {
                label: "Game Boy Advance (GBA)",
                value: "gba",
            },
            ConfigOption {
                label: "Sega Game Gear",
                value: "gamegear",
            },
            ConfigOption {
                label: "Atari Lynx",
                value: "lynx",
            },
            ConfigOption {
                label: "Neo-Geo Pocket Color",
                value: "ngp",
            },
            ConfigOption {
                label: "Atari 2600",
                value: "atari2600",
            },
            ConfigOption {
                label: "Atari 7800",
                value: "atari7800",
            },
            ConfigOption {
                label: "Commodore 64 (C64)",
                value: "c64",
            },
            ConfigOption {
                label: "Commodore Amiga",
                value: "amiga",
            },
            ConfigOption {
                label: "MSX / MSX2",
                value: "msx",
            },
            ConfigOption {
                label: "ColecoVision",
                value: "colecovision",
            },
            ConfigOption {
                label: "Mattel Intellivision",
                value: "intellivision",
            },
            ConfigOption {
                label: "GCE Vectrex",
                value: "vectrex",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Game Genres (16 Categories)",
        description: "Filter retro & arcade games by genre",
        default_value: "action,fighting,shootemup,runandgun,platform,beatemup,rpg,racing",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Action",
                value: "action",
            },
            ConfigOption {
                label: "Platformer",
                value: "platform",
            },
            ConfigOption {
                label: "Shoot 'em Up (Shmup)",
                value: "shootemup",
            },
            ConfigOption {
                label: "Fighting (1v1)",
                value: "fighting",
            },
            ConfigOption {
                label: "Beat 'em Up (Brawler)",
                value: "beatemup",
            },
            ConfigOption {
                label: "Run and Gun",
                value: "runandgun",
            },
            ConfigOption {
                label: "Role-Playing (RPG)",
                value: "rpg",
            },
            ConfigOption {
                label: "Racing",
                value: "racing",
            },
            ConfigOption {
                label: "Puzzle",
                value: "puzzle",
            },
            ConfigOption {
                label: "Sports",
                value: "sports",
            },
            ConfigOption {
                label: "Adventure",
                value: "adventure",
            },
            ConfigOption {
                label: "Strategy / Tactics",
                value: "strategy",
            },
            ConfigOption {
                label: "Pinball",
                value: "pinball",
            },
            ConfigOption {
                label: "Light Gun Shooter",
                value: "lightgun",
            },
            ConfigOption {
                label: "Maze Arcade",
                value: "maze",
            },
            ConfigOption {
                label: "Simulation",
                value: "simulation",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades",
        description: "Filter games by release era",
        default_value: "1980s,1990s,2000s",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "1970s",
                value: "1970s",
            },
            ConfigOption {
                label: "1980s",
                value: "1980s",
            },
            ConfigOption {
                label: "1990s",
                value: "1990s",
            },
            ConfigOption {
                label: "2000s",
                value: "2000s",
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
