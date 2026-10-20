//! Movie Showcase Engine (`movie_showcase`).
//!
//! Displays localized movie posters, ratings, synopses, backdrop pan-scans, and
//! AI-selected trailer highlights prepared by `dmd-media-showcase`.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::engines::renderers::media_showcase::{
    build_common_showcase_fields, MediaShowcaseRunner, ShowcaseDomain,
};
use linkme::distributed_slice;

pub struct MovieShowcaseEngine {
    runner: MediaShowcaseRunner,
}

impl MovieShowcaseEngine {
    pub fn new() -> Self {
        Self {
            runner: MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 32),
        }
    }
}

impl Default for MovieShowcaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for MovieShowcaseEngine {
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
fn register_movie_showcase_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("auto_action");
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Movie Genres",
        description: "Filter movies by genre (leave empty or select multiple)",
        default_value: "scifi,action,animation",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Sci-Fi",
                value: "scifi",
            },
            ConfigOption {
                label: "Action",
                value: "action",
            },
            ConfigOption {
                label: "Animation",
                value: "animation",
            },
            ConfigOption {
                label: "Horror",
                value: "horror",
            },
            ConfigOption {
                label: "Thriller",
                value: "thriller",
            },
            ConfigOption {
                label: "Adventure",
                value: "adventure",
            },
            ConfigOption {
                label: "Fantasy",
                value: "fantasy",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades",
        description: "Filter movies by release decade",
        default_value: "1980s,1990s,2000s,2010s,2020s",
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
            id: "movie_showcase",
            name: "MovieShowcase",
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
        factory: || Box::new(MovieShowcaseEngine::new()),
    }
}
