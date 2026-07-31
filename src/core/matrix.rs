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
    canvas: LedCanvas,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl HardwareMatrix {
    pub fn new(
        rows: u32,
        cols: u32,
        chain: u32,
        parallel: u32,
        hardware_mapping: &str,
        slowdown: u32,
        brightness: u8,
    ) -> Result<Self, String> {
        let mut options = LedMatrixOptions::new();
        options.set_rows(rows);
        options.set_cols(cols);
        options.set_chain_length(chain);
        options.set_parallel(parallel);
        if !hardware_mapping.is_empty() {
            options.set_hardware_mapping(hardware_mapping);
        }

        let mut rt_options = LedRuntimeOptions::new();
        rt_options.set_gpio_slowdown(slowdown as i32);

        let matrix = LedMatrix::new(options, rt_options)
            .map_err(|e| format!("Failed to init LED matrix: {:?}", e))?;
        let canvas = matrix.offscreen_canvas();

        let mut hw = Self { matrix, canvas };
        hw.set_brightness(brightness);
        Ok(hw)
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl MatrixBackend for HardwareMatrix {
    fn width(&self) -> u32 {
        self.canvas.size().0 as u32
    }

    fn height(&self) -> u32 {
        self.canvas.size().1 as u32
    }

    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8) {
        if x >= 0 && y >= 0 && x < self.width() as i32 && y < self.height() as i32 {
            self.canvas
                .set_pixel(x as usize, y as usize, red, green, blue);
        }
    }

    fn clear(&mut self) {
        self.canvas.clear();
    }

    fn update(&mut self) {
        self.canvas = self.matrix.swap_canvas(self.canvas);
    }

    fn set_brightness(&mut self, brightness: u8) {
        self.matrix.set_brightness(brightness);
    }
}
