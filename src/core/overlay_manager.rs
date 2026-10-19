use crate::core::config::{OverlayConfig, SystemConfig};
use crate::core::matrix::IDrawingSurface;
use crate::core::types::FighterOverride;
use crate::engines::fighter::FighterEngine;

/// Transverse composition layer applying decorative overlays additively onto base framebuffers.
///
/// CONTRACT:
/// - Overlay rendering MUST be additive onto existing framebuffers.
/// - Overlay rendering MUST NOT clear or replace the base display.
/// - FighterEngine is created once and preserved in memory to avoid continuous asset loading.
pub struct OverlayManager {
    fighter: FighterEngine,
    fighter_active: bool,
}

impl OverlayManager {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            fighter: FighterEngine::new(width, height),
            fighter_active: false,
        }
    }

    /// Configures which overlays should be active for the current frame.
    /// Follows the 3-tier resolution:
    /// - Level 1: Active engine must allow overlay (handled by caller passing empty if not allowed)
    /// - Level 2: Master switch (system_config.idle_fighter_enabled)
    /// - Level 3: Per-rotation entry toggle (overlays.fighter)
    pub fn configure(&mut self, overlays: &OverlayConfig, system_config: &SystemConfig) {
        let ov = if overlays.fighter {
            FighterOverride::Enabled
        } else {
            FighterOverride::Disabled
        };
        self.configure_with_override(ov, system_config);
    }

    /// Explicit tri-state override configuration (Unspecified, Enabled, Disabled)
    pub fn configure_with_override(
        &mut self,
        fighter_override: FighterOverride,
        system_config: &SystemConfig,
    ) {
        // Master switch invariant: if global switch is OFF, Fighter is NEVER active regardless of override.
        let should_be_active =
            system_config.idle_fighter_enabled && (fighter_override != FighterOverride::Disabled);

        if should_be_active {
            self.fighter
                .set_interval(system_config.idle_fighter_interval);
            self.fighter.set_speed(system_config.idle_fighter_speed);
            self.fighter_active = true;
        } else {
            if self.fighter.is_active() || self.fighter.is_loading() {
                self.fighter.stop();
            }
            self.fighter_active = false;
        }
    }

    /// Advances overlay schedulers and polls background sprite loading channels.
    /// Must be called before `is_active()` so newly scheduled/loaded fights activate
    /// before the frame's render/composite gate is evaluated.
    pub fn update(&mut self, width: u32, height: u32) {
        if self.fighter_active {
            self.fighter.update(width, height);
        }
    }

    /// Composite active overlays additively onto the base matrix framebuffer.
    pub fn composite(&mut self, surface: &mut dyn IDrawingSurface) {
        if self.fighter_active {
            self.fighter.composite(surface);
        }
    }

    /// Whether any overlay is currently active and animating (demands realtime cadence).
    pub fn is_active(&self) -> bool {
        self.fighter_active && self.fighter.is_active()
    }

    /// Whether a background overlay asset load is in flight (demands fast polling cadence).
    pub fn is_loading(&self) -> bool {
        self.fighter_active && self.fighter.is_loading()
    }

    /// Deactivates all overlays (e.g. during priority events or standby).
    pub fn deactivate(&mut self) {
        if self.fighter.is_active() || self.fighter.is_loading() {
            self.fighter.stop();
        }
        self.fighter_active = false;
    }

    /// Propagates display geometry/rotation updates to active overlays.
    pub fn on_display_geometry_changed(&mut self, geometry: &crate::core::types::DisplayGeometry) {
        self.fighter.on_display_geometry_changed(geometry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_manager_update_triggers_fight_without_deadlock() {
        let mut mgr = OverlayManager::new(128, 32);
        let ov = OverlayConfig { fighter: true };
        let mut sys = SystemConfig::default();
        sys.idle_fighter_enabled = true;
        sys.idle_fighter_interval = 1;

        mgr.configure(&ov, &sys);
        // Before update(), no fight has been scheduled yet
        assert!(!mgr.is_active());
        assert!(!mgr.is_loading());

        // Calling update() must schedule the fight (entering loading or active state)
        mgr.update(128, 32);
        assert!(mgr.is_loading() || mgr.is_active());
    }
}
