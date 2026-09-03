use crate::core::config::{OverlayConfig, SystemConfig};
use crate::core::matrix::MatrixBackend;
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
            if self.fighter.is_active() {
                self.fighter.stop();
            }
            self.fighter_active = false;
        }
    }

    /// Composite active overlays additively onto the base matrix framebuffer.
    pub fn composite(&mut self, matrix: &mut dyn MatrixBackend) {
        if self.fighter_active {
            self.fighter.composite(matrix);
        }
    }

    /// Whether any overlay is currently active and animating (demands realtime ~25fps cadence).
    pub fn is_active(&self) -> bool {
        self.fighter_active && self.fighter.is_active()
    }

    /// Deactivates all overlays (e.g. during priority events or standby).
    pub fn deactivate(&mut self) {
        if self.fighter.is_active() {
            self.fighter.stop();
        }
        self.fighter_active = false;
    }
}
