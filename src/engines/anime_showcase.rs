//! Anime Showcase Engine (`anime_showcase`).
//!
//! Displays localized anime covers, demographic/genre badges (Shonen, Seinen, Mecha),
//! localized synopses, and creditless Opening/Ending/PV video clips prepared by `dmd-media-showcase`.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::engines::renderers::media_showcase::{
    build_common_showcase_fields, MediaShowcaseRunner, ShowcaseDomain,
};
use linkme::distributed_slice;

pub struct AnimeShowcaseEngine {
    runner: MediaShowcaseRunner,
}

impl AnimeShowcaseEngine {
    pub fn new() -> Self {
        Self {
            runner: MediaShowcaseRunner::new(ShowcaseDomain::Anime, 128, 32),
        }
    }
}

impl Default for AnimeShowcaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for AnimeShowcaseEngine {
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
fn register_anime_showcase_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("anime");
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Anime Genres",
        description: "Filter anime by genre or demographic",
        default_value: "shonen,seinen,mecha,cyberpunk",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Shonen",
                value: "shonen",
            },
            ConfigOption {
                label: "Seinen",
                value: "seinen",
            },
            ConfigOption {
                label: "Mecha",
                value: "mecha",
            },
            ConfigOption {
                label: "Cyberpunk & Sci-Fi",
                value: "cyberpunk",
            },
            ConfigOption {
                label: "Fantasy & Adventure",
                value: "fantasy",
            },
            ConfigOption {
                label: "Action",
                value: "action",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "video_type",
        field_type: ConfigType::Options,
        label: "Preferred Video Type",
        description:
            "Select creditless Opening (OP), Ending (ED), Promotional Trailer (PV), or Any",
        default_value: "any",
        options: Some(vec![
            ConfigOption {
                label: "Any (OP / ED / PV)",
                value: "any",
            },
            ConfigOption {
                label: "Opening (OP)",
                value: "opening",
            },
            ConfigOption {
                label: "Ending (ED)",
                value: "ending",
            },
            ConfigOption {
                label: "Promotional Trailer (PV)",
                value: "pv",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });

    EngineDescriptor {
        metadata: EngineMetadata {
            id: "anime_showcase",
            name: "AnimeShowcase",
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
        factory: || Box::new(AnimeShowcaseEngine::new()),
    }
}
