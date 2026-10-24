//! TV Series Showcase Engine (`series_showcase`).
//!
//! Displays localized TV series covers, network/era badges, complete scrolling synopses,
//! granular panoramas (`jacket_panorama`, `marquee_panorama`, `snapshot_panorama_count`),
//! and language-agnostic opening/trailer clips prepared live by `dmd-media-showcase`.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
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

    fn set_rotation_budget(&mut self, budget: u32) {
        self.runner.set_rotation_budget(budget);
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
        label: "TV Series Genres (Dynamic Referential)",
        description:
            "Filter live TV series discovery by genre fetched dynamically from dmd-media-showcase",
        default_value: "scifi,drama,crime,mystery,thriller,action,fantasy",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/series/filters"),
        validation_policy: ValidationPolicy::Accept,
        ..Default::default()
    });
    fields.push(ConfigField {
        id: "decades",
        field_type: ConfigType::Options,
        label: "Decades (Dynamic Referential)",
        description:
            "Filter TV series by premiere decade fetched dynamically from dmd-media-showcase",
        default_value: "1990s,2000s,2010s,2020s",
        multiple: true,
        options: None,
        options_endpoint: Some("/api/v1/series/filters"),
        validation_policy: ValidationPolicy::Accept,
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
