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
use rpi_led_matrix::{LedCanvas, LedColor, LedMatrix, LedMatrixOptions, LedRuntimeOptions};

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
pub struct HardwareMatrix {
    matrix: LedMatrix,
    canvas: Option<LedCanvas>,
    brightness: u8,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Send for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Sync for HardwareMatrix {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl HardwareMatrix {
    pub fn new(
        rows: u32,
        cols: u32,
        chain: u32,
        parallel: u32,
        hardware_mapping: &str,
        rgb_sequence: &str,
        slowdown: u32,
        pwm_bits: u32,
        pwm_lsb: u32,
        disable_hardware_pulsing: bool,
        brightness: u8,
    ) -> Result<Self, String> {
        let mut options = LedMatrixOptions::new();
        options.set_rows(rows);
        options.set_cols(cols);
        options.set_chain_length(chain);
        options.set_parallel(parallel);
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

        let mut rt_options = LedRuntimeOptions::new();
        rt_options.set_gpio_slowdown(slowdown);
        rt_options.set_drop_privileges(false);

        let matrix = LedMatrix::new(Some(options), Some(rt_options))
            .map_err(|e| format!("Failed to init LED matrix: {:?}", e))?;
        let canvas = matrix.offscreen_canvas();

        Ok(Self {
            matrix,
            canvas: Some(canvas),
            brightness,
        })
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl MatrixBackend for HardwareMatrix {
    fn width(&self) -> u32 {
        self.canvas.as_ref().unwrap().canvas_size().0 as u32
    }

    fn height(&self) -> u32 {
        self.canvas.as_ref().unwrap().canvas_size().1 as u32
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        if x >= 0 && y >= 0 && x < self.width() as i32 && y < self.height() as i32 {
            let color = LedColor { red, green, blue };
            self.canvas.as_mut().unwrap().set(x, y, &color);
        }
    }

    fn clear(&mut self) {
        self.canvas.as_mut().unwrap().clear();
    }

    fn update(&mut self) {
        let canvas = self.canvas.take().unwrap();
        self.canvas = Some(self.matrix.swap(canvas));
    }

    fn set_brightness(&mut self, _brightness: u8) {
        // Hardware brightness cannot be changed dynamically via this Rust wrapper without access to the internal handle.
        // The application handles dynamic changes by triggering a graceful restart (reload_flag)
        // when brightness is changed via UI or MQTT, which re-initializes the matrix with the new brightness.
    }
}
