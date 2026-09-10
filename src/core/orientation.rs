use crate::core::types::DisplayGeometry;

/// Platform-independent architectural orientation manager
/// On ESP32, orientation can be fed by IMU/gyroscope.
/// On RPi, orientation is fed by manual configuration/API.
/// Both emit the identical DisplayGeometry contract with monotonic versioning.
pub struct OrientationManager {
    geometry: DisplayGeometry,
}

impl OrientationManager {
    pub fn new(physical_width: u32, physical_height: u32, initial_rotation: u8) -> Self {
        let geometry = DisplayGeometry::new(physical_width, physical_height, initial_rotation, 1);
        Self { geometry }
    }

    #[inline]
    pub fn geometry(&self) -> &DisplayGeometry {
        &self.geometry
    }

    /// Updates manual rotation setting (0: 0°, 1: 90°, 2: 180°, 3: 270°).
    /// Increments geometry version on change and returns true.
    pub fn set_rotation(&mut self, rotation: u8) -> bool {
        let normalized = rotation % 4;
        if self.geometry.rotation != normalized {
            let next_version = self.geometry.version.wrapping_add(1);
            self.geometry = DisplayGeometry::new(
                self.geometry.physical_width,
                self.geometry.physical_height,
                normalized,
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
            self.geometry = DisplayGeometry::new(
                width,
                height,
                self.geometry.rotation,
                if next_version == 0 { 1 } else { next_version },
            );
            true
        } else {
            false
        }
    }
}
