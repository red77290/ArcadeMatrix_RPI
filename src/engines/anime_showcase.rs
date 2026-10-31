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

    fn on_display_geometry_changed(&mut self, geometry: &crate::core::types::DisplayGeometry) {
        self.runner
            .on_display_geometry_changed(geometry.width(), geometry.height());
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

    fn allows_overlay(&self) -> bool {
        self.runner.allows_overlay()
    }

    fn set_rotation_budget(&mut self, budget: u32) {
        self.runner.set_rotation_budget(budget);
    }

    fn is_finished(&self) -> bool {
        self.runner.is_finished()
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_anime_showcase_engine() -> EngineDescriptor {
    let mut fields = build_common_showcase_fields("auto_action");
    fields.push(ConfigField {
        id: "genres",
        field_type: ConfigType::Options,
        label: "Anime Genres & Themes (Dynamic Referential)",
        description: "Filter live anime discovery by genre, theme, or demographic fetched dynamically from dmd-media-showcase",
        default_value: "shonen,seinen,mecha,cyberpunk,action,fantasy,scifi",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/anime/filters"),
        validation_policy: ValidationPolicy::Accept,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades (Dynamic Referential)",
        description: "Filter anime by release decade fetched dynamically from dmd-media-showcase",
        default_value: "1980s,1990s,2000s,2010s,2020s",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/anime/filters"),
        validation_policy: ValidationPolicy::Accept,
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
