//! TV Series Showcase Engine (`series_showcase`).
//!
//! Displays localized TV series covers, network/era badges, complete scrolling synopses,
//! granular panoramas (`jacket_panorama`, `marquee_panorama`, `snapshot_panorama_count`),
//! and language-agnostic opening/trailer clips prepared live by `dmd-media-showcase`.

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
        label: "TV Series Genres (24 Categories)",
        description: "Filter live TV series discovery by genre (aligned with TVMaze & TMDB)",
        default_value: "scifi,drama,crime,mystery,thriller,action,fantasy",
        multiple: true,
        options: Some(vec![
            ConfigOption {
                label: "Science-Fiction",
                value: "scifi",
            },
            ConfigOption {
                label: "Cyberpunk",
                value: "cyberpunk",
            },
            ConfigOption {
                label: "Action",
                value: "action",
            },
            ConfigOption {
                label: "Adventure",
                value: "adventure",
            },
            ConfigOption {
                label: "Drama",
                value: "drama",
            },
            ConfigOption {
                label: "Crime",
                value: "crime",
            },
            ConfigOption {
                label: "Thriller",
                value: "thriller",
            },
            ConfigOption {
                label: "Mystery",
                value: "mystery",
            },
            ConfigOption {
                label: "Supernatural",
                value: "supernatural",
            },
            ConfigOption {
                label: "Fantasy",
                value: "fantasy",
            },
            ConfigOption {
                label: "Horror",
                value: "horror",
            },
            ConfigOption {
                label: "Comedy",
                value: "comedy",
            },
            ConfigOption {
                label: "Espionage",
                value: "espionage",
            },
            ConfigOption {
                label: "Animation / Anime",
                value: "anime",
            },
            ConfigOption {
                label: "History",
                value: "history",
            },
            ConfigOption {
                label: "War",
                value: "war",
            },
            ConfigOption {
                label: "Western",
                value: "western",
            },
            ConfigOption {
                label: "Medical",
                value: "medical",
            },
            ConfigOption {
                label: "Legal",
                value: "legal",
            },
            ConfigOption {
                label: "Romance",
                value: "romance",
            },
            ConfigOption {
                label: "Family",
                value: "family",
            },
            ConfigOption {
                label: "Music",
                value: "music",
            },
            ConfigOption {
                label: "Sports",
                value: "sports",
            },
            ConfigOption {
                label: "Nature / Documentary",
                value: "nature",
            },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades",
        description: "Filter TV series by premiere decade",
        default_value: "1990s,2000s,2010s,2020s",
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
