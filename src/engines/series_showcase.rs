//! TV Series Showcase Engine (`series_showcase`).
//!
//! Displays localized TV series covers, network/era badges, synopses, backdrop pan-scans,
//! and opening/promo trailer clips prepared by `dmd-media-showcase`.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::engines::renderers::media_showcase::{
    build_common_showcase_fields, MediaShowcaseRunner, ShowcaseDomain,
};
use linkme::distributed_slice;

pub struct SeriesShowcaseEngine {
    runner: MediaShowcaseRunner,
}

impl SeriesShowcaseEngine {
    pub fn new() -> Self {
        Self {
            runner: MediaShowcaseRunner::new(ShowcaseDomain::Series, 128, 32),
        }
    }
}

impl Default for SeriesShowcaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for SeriesShowcaseEngine {
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
fn register_series_showcase_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("auto_action");
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Series Genres",
        description: "Filter TV series by genre",
        default_value: "scifi,drama,crime,mystery",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Sci-Fi & Fantasy",
                value: "scifi",
            },
            ConfigOption {
                label: "Drama",
                value: "drama",
            },
            ConfigOption {
                label: "Crime & Thriller",
                value: "crime",
            },
            ConfigOption {
                label: "Mystery & Paranormal",
                value: "mystery",
            },
            ConfigOption {
                label: "Action & Adventure",
                value: "action",
            },
            ConfigOption {
                label: "Comedy",
                value: "comedy",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades",
        description: "Filter TV series by release decade",
        default_value: "1990s,2000s,2010s,2020s",
        multiple: true,
        options: Some(vec![
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
            ConfigOption {
                label: "2010s",
                value: "2010s",
            },
            ConfigOption {
                label: "2020s",
                value: "2020s",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });

    EngineDescriptor {
        metadata: EngineMetadata {
            id: "series_showcase",
            name: "SeriesShowcase",
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
        factory: || Box::new(SeriesShowcaseEngine::new()),
    }
}
