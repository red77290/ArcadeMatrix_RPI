//! Anime Showcase Engine (`anime_showcase`).
//!
//! Displays localized anime covers, demographic/genre badges, complete scrolling synopses,
//! granular panoramas (`jacket_panorama`, `marquee_panorama`, `snapshot_panorama_count`),
//! and creditless Opening/Ending/PV video clips prepared live by `dmd-media-showcase`.

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
        label: "Anime Genres & Themes (31 Categories)",
        description: "Filter live anime discovery by genre, theme, or demographic (Jikan / Kitsu / AnimeThemes)",
        default_value: "shonen,seinen,mecha,cyberpunk,action,fantasy,scifi",
        multiple: true,
        options: Some(vec![
            ConfigOption { label: "Shonen", value: "shonen" },
            ConfigOption { label: "Seinen", value: "seinen" },
            ConfigOption { label: "Shojo", value: "shojo" },
            ConfigOption { label: "Action", value: "action" },
            ConfigOption { label: "Adventure", value: "adventure" },
            ConfigOption { label: "Mecha", value: "mecha" },
            ConfigOption { label: "Cyberpunk", value: "cyberpunk" },
            ConfigOption { label: "Sci-Fi", value: "scifi" },
            ConfigOption { label: "Fantasy", value: "fantasy" },
            ConfigOption { label: "Isekai", value: "isekai" },
            ConfigOption { label: "Supernatural", value: "supernatural" },
            ConfigOption { label: "Psychological", value: "psychological" },
            ConfigOption { label: "Mystery", value: "mystery" },
            ConfigOption { label: "Thriller / Suspense", value: "thriller" },
            ConfigOption { label: "Horror", value: "horror" },
            ConfigOption { label: "Martial Arts", value: "martial_arts" },
            ConfigOption { label: "Samurai", value: "samurai" },
            ConfigOption { label: "Space Opera", value: "space" },
            ConfigOption { label: "Post-Apocalyptic", value: "post_apocalyptic" },
            ConfigOption { label: "Military", value: "military" },
            ConfigOption { label: "Historical", value: "historical" },
            ConfigOption { label: "Comedy", value: "comedy" },
            ConfigOption { label: "Drama", value: "drama" },
            ConfigOption { label: "Romance", value: "romance" },
            ConfigOption { label: "Slice of Life", value: "slice_of_life" },
            ConfigOption { label: "Sports", value: "sports" },
            ConfigOption { label: "Racing / Motorsports", value: "racing" },
            ConfigOption { label: "Music", value: "music" },
            ConfigOption { label: "Magical Girl (Mahou Shoujo)", value: "mahou_shoujo" },
            ConfigOption { label: "Vampire", value: "vampire" },
            ConfigOption { label: "Parody", value: "parody" },
        ]),
        validation_policy: ValidationPolicy::FallbackDefault,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades",
        description: "Filter anime by release decade",
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
