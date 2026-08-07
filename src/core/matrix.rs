use image::RgbImage;

pub trait MatrixBackend: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8);
    fn clear(&mut self);
    fn update(&mut self);
    fn set_brightness(&mut self, brightness: u8);
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
    pub width: u32,
    pub height: u32,
    pub brightness: u8,
    pub canvas: RgbImage,
}

impl MockMatrix {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            brightness: 100,
            canvas: RgbImage::new(width, height),
        }
    }
}

impl MatrixBackend for MockMatrix {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
            let scale = self.brightness as f32 / 100.0;
            let r = (red as f32 * scale) as u8;
            let g = (green as f32 * scale) as u8;
            let b = (blue as f32 * scale) as u8;
            self.canvas
                .put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
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
    width: u32,
    height: u32,
    buffer: image::RgbImage,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Send for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Sync for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
extern "C" {
    fn led_matrix_set_brightness(matrix: *mut std::ffi::c_void, brightness: u8);
    // Bulk pixel upload from the hzeller C API. Copies the whole RGB buffer in a
    // single FFI crossing (the inner per-pixel loop runs in C++), instead of
    // issuing one FFI call per pixel. This mirrors Python's canvas.SetImage()
    // and is ~1000x fewer FFI crossings per frame, which keeps the render thread
    // light enough that the web API and Wi-Fi DMA are no longer starved.
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

        // Pass limit_refresh directly from conf.ini, no override.
        // Python doesn't set this at all (defaults to 0 in the C++ lib).
        options.set_limit_refresh(limit_refresh);

        // WORKAROUND for hzeller/rpi-rgb-led-matrix bug:
        // The C API wrapper `led-matrix-c.cc` uses a macro `if (rt_opts->drop_privileges)`
        // which ignores the value `0` (false), meaning `rt_options.set_drop_privileges(false)`
        // is completely ignored and it defaults to dropping privileges to `daemon` (uid 1).
        // To prevent this, we trick the library into dropping privileges to root (uid 0)
        // by faking the SUDO_UID and SUDO_GID environment variables.
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
            width: w as u32,
            height: h as u32,
            buffer: image::RgbImage::new(w as u32, h as u32),
        })
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl MatrixBackend for HardwareMatrix {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.buffer
                .put_pixel(x as u32, y as u32, image::Rgb([red, green, blue]));
        }
    }

    fn draw_image(&mut self, img: &RgbImage, offset_x: i32, offset_y: i32) {
        let (w, h) = img.dimensions();
        for dy in 0..h {
            let py = offset_y + dy as i32;
            if py >= 0 && py < self.height as i32 {
                for dx in 0..w {
                    let px = offset_x + dx as i32;
                    if px >= 0 && px < self.width as i32 {
                        self.buffer
                            .put_pixel(px as u32, py as u32, *img.get_pixel(dx, dy));
                    }
                }
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
                        self.width as std::ffi::c_int,
                        self.height as std::ffi::c_int,
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
            return; // Avoid hammering the C++ library
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
