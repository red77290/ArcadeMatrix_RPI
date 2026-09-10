use image::RgbImage;

pub trait MatrixBackend: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8);
    fn clear(&mut self);
    fn update(&mut self);
    fn set_brightness(&mut self, brightness: u8);
    fn set_rotation(&mut self, _rotation: u8) {}
    fn rotation(&self) -> u8 {
        0
    }
    fn draw_image(&mut self, img: &RgbImage, offset_x: i32, offset_y: i32) {
        let (img_w, img_h) = img.dimensions();
        for iy in 0..img_h {
            for ix in 0..img_w {
                let px = img.get_pixel(ix, iy);
                self.set_pixel(
                    offset_x + ix as i32,
                    offset_y + iy as i32,
                    px[0],
                    px[1],
                    px[2],
                );
            }
        }
    }
}

pub struct MockMatrix {
    pub physical_width: u32,
    pub physical_height: u32,
    pub rotation: u8,
    pub brightness: u8,
    pub canvas: RgbImage,
}

impl MockMatrix {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            physical_width: width,
            physical_height: height,
            rotation: 0,
            brightness: 100,
            canvas: RgbImage::new(width, height),
        }
    }
}

impl MatrixBackend for MockMatrix {
    fn width(&self) -> u32 {
        if self.rotation % 2 == 1 {
            self.physical_height
        } else {
            self.physical_width
        }
    }

    fn height(&self) -> u32 {
        if self.rotation % 2 == 1 {
            self.physical_width
        } else {
            self.physical_height
        }
    }

    fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
    }

    fn rotation(&self) -> u8 {
        self.rotation
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        let log_w = self.width() as i32;
        let log_h = self.height() as i32;
        if x < 0 || y < 0 || x >= log_w || y >= log_h {
            return;
        }

        let (phys_x, phys_y) = match self.rotation {
            1 => (self.physical_width as i32 - 1 - y, x),
            2 => (
                self.physical_width as i32 - 1 - x,
                self.physical_height as i32 - 1 - y,
            ),
            3 => (y, self.physical_height as i32 - 1 - x),
            _ => (x, y),
        };

        if phys_x >= 0
            && phys_x < self.physical_width as i32
            && phys_y >= 0
            && phys_y < self.physical_height as i32
        {
            let scale = self.brightness as f32 / 100.0;
            let r = (red as f32 * scale) as u8;
            let g = (green as f32 * scale) as u8;
            let b = (blue as f32 * scale) as u8;
            self.canvas
                .put_pixel(phys_x as u32, phys_y as u32, image::Rgb([r, g, b]));
        }
    }

    fn clear(&mut self) {
        for px in self.canvas.pixels_mut() {
            *px = image::Rgb([0, 0, 0]);
        }
    }

    fn update(&mut self) {
        // Mock matrix: buffer ready
    }

    fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(100);
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use rpi_led_matrix::{LedCanvas, LedMatrix, LedMatrixOptions, LedRuntimeOptions};

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
pub struct HardwareMatrix {
    matrix: LedMatrix,
    canvas: Option<LedCanvas>,
    brightness: u8,
    physical_width: u32,
    physical_height: u32,
    rotation: u8,
    buffer: image::RgbImage,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Send for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Sync for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
extern "C" {
    fn led_matrix_set_brightness(matrix: *mut std::ffi::c_void, brightness: u8);
    fn set_image(
        canvas: *mut std::ffi::c_void,
        canvas_offset_x: std::ffi::c_int,
        canvas_offset_y: std::ffi::c_int,
        image_buffer: *const u8,
        buffer_size_bytes: usize,
        image_width: std::ffi::c_int,
        image_height: std::ffi::c_int,
        is_bgr: std::ffi::c_char,
    );
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl HardwareMatrix {
    pub fn new(
        rows: u32,
        cols: u32,
        chain: u32,
        parallel: u32,
        multiplexing: u32,
        row_addr_type: u32,
        hardware_mapping: &str,
        rgb_sequence: &str,
        slowdown: u32,
        pwm_bits: u32,
        pwm_lsb: u32,
        disable_hardware_pulsing: bool,
        brightness: u8,
        limit_refresh: u32,
        driver_chip: &str,
    ) -> Result<Self, String> {
        let mut options = LedMatrixOptions::new();
        options.set_rows(rows);
        options.set_cols(cols);
        options.set_chain_length(chain);
        options.set_parallel(parallel);
        options.set_multiplexing(multiplexing);
        options.set_row_addr_type(row_addr_type);
        options
            .set_brightness(brightness.min(100).max(1))
            .unwrap_or(());
        if !hardware_mapping.is_empty() {
            options.set_hardware_mapping(hardware_mapping);
        }
        if !rgb_sequence.is_empty() {
            options.set_led_rgb_sequence(rgb_sequence);
        }
        let _ = options.set_pwm_bits(pwm_bits as u8);
        options.set_pwm_lsb_nanoseconds(pwm_lsb);
        options.set_hardware_pulsing(!disable_hardware_pulsing);
        if !driver_chip.is_empty() {
            let chip_lower = driver_chip.to_lowercase();
            if chip_lower == "fm6126a" || chip_lower == "fm6127" {
                options.set_panel_type(driver_chip);
            }
        }

        options.set_limit_refresh(limit_refresh);
        options.set_refresh_rate(false);

        std::env::set_var("SUDO_UID", "0");
        std::env::set_var("SUDO_GID", "0");

        let mut rt_options = LedRuntimeOptions::new();
        rt_options.set_gpio_slowdown(slowdown);
        rt_options.set_drop_privileges(false);

        let matrix = LedMatrix::new(Some(options), Some(rt_options))
            .map_err(|e| format!("Failed to init LED matrix: {:?}", e))?;
        let canvas = matrix.offscreen_canvas();
        let (w, h) = canvas.canvas_size();

        Ok(Self {
            matrix,
            canvas: Some(canvas),
            brightness,
            physical_width: w as u32,
            physical_height: h as u32,
            rotation: 0,
            buffer: image::RgbImage::new(w as u32, h as u32),
        })
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl MatrixBackend for HardwareMatrix {
    fn width(&self) -> u32 {
        if self.rotation % 2 == 1 {
            self.physical_height
        } else {
            self.physical_width
        }
    }

    fn height(&self) -> u32 {
        if self.rotation % 2 == 1 {
            self.physical_width
        } else {
            self.physical_height
        }
    }

    fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
    }

    fn rotation(&self) -> u8 {
        self.rotation
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        let log_w = self.width() as i32;
        let log_h = self.height() as i32;
        if x < 0 || y < 0 || x >= log_w || y >= log_h {
            return;
        }

        let (phys_x, phys_y) = match self.rotation {
            1 => (self.physical_width as i32 - 1 - y, x),
            2 => (
                self.physical_width as i32 - 1 - x,
                self.physical_height as i32 - 1 - y,
            ),
            3 => (y, self.physical_height as i32 - 1 - x),
            _ => (x, y),
        };

        if phys_x >= 0
            && phys_y >= 0
            && phys_x < self.physical_width as i32
            && phys_y < self.physical_height as i32
        {
            self.buffer
                .put_pixel(phys_x as u32, phys_y as u32, image::Rgb([red, green, blue]));
        }
    }

    fn draw_image(&mut self, img: &RgbImage, offset_x: i32, offset_y: i32) {
        let (w, h) = img.dimensions();
        for dy in 0..h {
            let py = offset_y + dy as i32;
            for dx in 0..w {
                let px = offset_x + dx as i32;
                let c = img.get_pixel(dx, dy);
                self.set_pixel(px, py, c[0], c[1], c[2]);
            }
        }
    }

    fn clear(&mut self) {
        self.buffer.as_flat_samples_mut().as_mut_slice().fill(0);
    }

    fn update(&mut self) {
        if let Some(ref canvas) = self.canvas {
            let handle = unsafe {
                *(canvas as *const rpi_led_matrix::LedCanvas as *const *mut std::ffi::c_void)
            };
            if !handle.is_null() {
                unsafe {
                    set_image(
                        handle,
                        0,
                        0,
                        self.buffer.as_raw().as_ptr(),
                        self.buffer.len(),
                        self.physical_width as std::ffi::c_int,
                        self.physical_height as std::ffi::c_int,
                        0, // is_bgr = false
                    );
                }
            }
        }

        let canvas = self.canvas.take().unwrap();
        self.canvas = Some(self.matrix.swap(canvas));
    }

    fn set_brightness(&mut self, brightness: u8) {
        let new_b = brightness.min(100);
        if self.brightness == new_b {
            return;
        }
        self.brightness = new_b;

        let matrix_ptr = &self.matrix as *const _ as *const *mut std::ffi::c_void;
        unsafe {
            let handle = *matrix_ptr;
            if !handle.is_null() {
                led_matrix_set_brightness(handle, self.brightness);
            }
        }
    }
}
