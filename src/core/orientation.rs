use crate::core::types::DisplayGeometry;

/// Platform-independent architectural orientation manager
/// On ESP32, orientation can be fed by IMU/gyroscope.
/// On RPi, orientation is fed by manual configuration/API.
/// Both emit the identical DisplayGeometry contract with monotonic versioning.
pub struct OrientationManager {
    geometry: DisplayGeometry,
    ui_scale: u32,
}

impl OrientationManager {
    pub fn new(physical_width: u32, physical_height: u32, initial_rotation: u8) -> Self {
        Self::new_with_scale(physical_width, physical_height, initial_rotation, 1)
    }

    pub fn new_with_scale(
        physical_width: u32,
        physical_height: u32,
        initial_rotation: u8,
        ui_scale: u32,
    ) -> Self {
        let scale = ui_scale.clamp(1, 4);
        let geometry = DisplayGeometry::new_scaled(
            physical_width,
            physical_height,
            initial_rotation % 4,
            scale,
            1,
        );
        Self {
            geometry,
            ui_scale: scale,
        }
    }

    #[inline]
    pub fn geometry(&self) -> &DisplayGeometry {
        &self.geometry
    }

    #[inline]
    pub fn ui_scale(&self) -> u32 {
        self.ui_scale
    }

    /// Updates active integer `ui_scale` factor (`1..=4`). Increments geometry version on change.
    pub fn set_ui_scale(&mut self, ui_scale: u32) -> bool {
        let scale = ui_scale.clamp(1, 4);
        if self.ui_scale != scale {
            self.ui_scale = scale;
            let next_version = self.geometry.version.wrapping_add(1);
            self.geometry = DisplayGeometry::new_scaled(
                self.geometry.physical_width,
                self.geometry.physical_height,
                self.geometry.rotation,
                scale,
                if next_version == 0 { 1 } else { next_version },
            );
            true
        } else {
            false
        }
    }

    /// Updates manual rotation setting (0: 0°, 1: 90°, 2: 180°, 3: 270°).
    /// Increments geometry version on change and returns true.
    pub fn set_rotation(&mut self, rotation: u8) -> bool {
        let normalized = rotation % 4;
        if self.geometry.rotation != normalized {
            let next_version = self.geometry.version.wrapping_add(1);
            self.geometry = DisplayGeometry::new_scaled(
                self.geometry.physical_width,
                self.geometry.physical_height,
                normalized,
                self.ui_scale,
                if next_version == 0 { 1 } else { next_version },
            );
            true
        } else {
            false
        }
    }

    /// Reconfigures physical display bounds. Increments geometry version on change.
    pub fn set_physical_dimensions(&mut self, width: u32, height: u32) -> bool {
        if self.geometry.physical_width != width || self.geometry.physical_height != height {
            let next_version = self.geometry.version.wrapping_add(1);
            self.geometry = DisplayGeometry::new_scaled(
                width,
                height,
                self.geometry.rotation,
                self.ui_scale,
                if next_version == 0 { 1 } else { next_version },
            );
            true
        } else {
            false
        }
    }
}
