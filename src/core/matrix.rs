use crate::core::config::MatrixConfig;
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use crate::core::config::RenderPipeline;
use image::RgbImage;

/// Bounded rectangular vertical span tracking dirty pixel regions for zero-copy row transfers.
#[derive(Debug, Clone)]
pub struct DirtyBounds {
    pub dirty: bool,
    pub min_y: u32,
    pub max_y: u32,
}

impl DirtyBounds {
    pub fn new(height: u32) -> Self {
        Self {
            dirty: false,
            min_y: height,
            max_y: 0,
        }
    }

    pub fn reset(&mut self, height: u32) {
        self.dirty = false;
        self.min_y = height;
        self.max_y = 0;
    }

    pub fn mark(&mut self, y: u32) {
        self.dirty = true;
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    pub fn mark_all(&mut self, height: u32) {
        self.dirty = true;
        self.min_y = 0;
        self.max_y = height.saturating_sub(1);
    }
}

/// Scoped borrowed view of an off-screen linear RGB canvas.
pub struct CanvasView<'a> {
    pub data: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub stride_bytes: usize,
}

/// Canonical Hardware-Agnostic Drawing SPI for ArcadeMatrix.
///
/// Decouples display engines from physical hardware buffering, DMA topologies,
/// and refresh modulation by providing an abstract rendering surface.
pub trait IDrawingSurface: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn physical_width(&self) -> u32;
    fn physical_height(&self) -> u32;
    fn set_rotation(&mut self, rotation: u8);
    fn rotation(&self) -> u8;
    fn set_pixel(&mut self, x: i32, y: i32, red: u8, green: u8, blue: u8);
    fn clear(&mut self);
    /// Commit rendered frame to hardware display.
    /// This is the canonical presentation method (no legacy update()).
    fn present(&mut self);
    fn set_brightness(&mut self, brightness: u8);
    fn mark_dirty(&mut self);
    fn is_dirty(&self) -> bool;

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

    fn blit565(
        &mut self,
        src: &[u16],
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        stride_pixels: Option<usize>,
    ) {
        let stride = stride_pixels.unwrap_or(w as usize);
        for row in 0..h {
            let row_offset = row as usize * stride;
            for col in 0..w {
                let pixel = src[row_offset + col as usize];
                let r = (((pixel >> 11) & 0x1F) * 255 / 31) as u8;
                let g = (((pixel >> 5) & 0x3F) * 255 / 63) as u8;
                let b = ((pixel & 0x1F) * 255 / 31) as u8;
                self.set_pixel(x + col as i32, y + row as i32, r, g, b);
            }
        }
    }

    fn acquire_canvas(&mut self) -> Option<CanvasView<'_>> {
        None
    }

    fn release_canvas(&mut self) {}

    fn has_canvas(&self) -> bool {
        false
    }
}

/// Move-only RAII guard strictly enforcing the scoped per-frame lifetime of a borrowed CanvasView.
pub struct ScopedCanvasGuard<'a> {
    surface: &'a mut dyn IDrawingSurface,
    released: bool,
}

impl<'a> ScopedCanvasGuard<'a> {
    pub fn new(surface: &'a mut dyn IDrawingSurface) -> Option<Self> {
        if surface.has_canvas() {
            Some(Self {
                surface,
                released: false,
            })
        } else {
            None
        }
    }

    pub fn mark_drawn_and_release(mut self) {
        if !self.released {
            self.surface.mark_dirty();
            self.surface.release_canvas();
            self.released = true;
        }
    }
}

impl<'a> Drop for ScopedCanvasGuard<'a> {
    fn drop(&mut self) {
        if !self.released {
            self.surface.release_canvas();
            self.released = true;
        }
    }
}

/// Software Mock Surface for off-target development, CI, and unit testing.
pub struct MockSurface {
    pub physical_width: u32,
    pub physical_height: u32,
    pub rotation: u8,
    pub brightness: u8,
    pub canvas: RgbImage,
    pub dirty_bounds: DirtyBounds,
}

impl MockSurface {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            physical_width: width,
            physical_height: height,
            rotation: 0,
            brightness: 100,
            canvas: RgbImage::new(width, height),
            dirty_bounds: DirtyBounds::new(height),
        }
    }
}

impl IDrawingSurface for MockSurface {
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

    fn physical_width(&self) -> u32 {
        self.physical_width
    }

    fn physical_height(&self) -> u32 {
        self.physical_height
    }

    fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
        self.dirty_bounds.mark_all(self.physical_height);
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
            let b_scale = self.brightness as u16;
            let r = ((red as u16 * b_scale) / 100) as u8;
            let g = ((green as u16 * b_scale) / 100) as u8;
            let b = ((blue as u16 * b_scale) / 100) as u8;
            self.canvas
                .put_pixel(phys_x as u32, phys_y as u32, image::Rgb([r, g, b]));
            self.dirty_bounds.mark(phys_y as u32);
        }
    }

    fn clear(&mut self) {
        for px in self.canvas.pixels_mut() {
            *px = image::Rgb([0, 0, 0]);
        }
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn present(&mut self) {
        self.dirty_bounds.reset(self.physical_height);
    }

    fn set_brightness(&mut self, brightness: u8) {
        let new_b = brightness.min(100);
        if self.brightness != new_b {
            self.brightness = new_b;
            self.dirty_bounds.mark_all(self.physical_height);
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn is_dirty(&self) -> bool {
        self.dirty_bounds.dirty
    }

    fn acquire_canvas(&mut self) -> Option<CanvasView<'_>> {
        let w = self.physical_width;
        let h = self.physical_height;
        let stride = (w * 3) as usize;
        Some(CanvasView {
            data: self.canvas.as_flat_samples_mut().samples,
            width: w,
            height: h,
            stride_bytes: stride,
        })
    }

    fn release_canvas(&mut self) {
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn has_canvas(&self) -> bool {
        true
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use rpi_led_matrix::{LedCanvas, LedMatrix, LedMatrixOptions, LedRuntimeOptions};

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
const _: () = assert!(
    std::mem::size_of::<LedCanvas>() == std::mem::size_of::<*mut std::ffi::c_void>()
        && std::mem::align_of::<LedCanvas>() == std::mem::align_of::<*mut std::ffi::c_void>(),
    "LedCanvas must be layout-identical to a single raw pointer"
);

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
extern "C" {
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

/// Canvas Buffered Surface (CanvasSingle or CanvasDouble).
/// Single buffer mode tracks dirty row bounds to push only updated spans,
/// avoiding full-frame memory bus saturation that starves the Wi-Fi SDIO controller.
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
pub struct CanvasBufferedSurface {
    matrix: LedMatrix,
    canvas: Option<LedCanvas>,
    brightness: u8,
    physical_width: u32,
    physical_height: u32,
    rotation: u8,
    buffer: RgbImage,
    scaled_buffer: Vec<u8>,
    dirty_bounds: DirtyBounds,
    pipeline: RenderPipeline,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Send for CanvasBufferedSurface {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Sync for CanvasBufferedSurface {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl CanvasBufferedSurface {
    pub fn new(cfg: &MatrixConfig, pipeline: RenderPipeline) -> Result<Self, String> {
        let double_buffered = pipeline == RenderPipeline::CanvasDouble;
        let (matrix, canvas, w, h) = init_led_matrix(cfg, double_buffered)?;
        let physical_width = w as u32;
        let physical_height = h as u32;
        let scaled_len = (physical_width * physical_height * 3) as usize;

        Ok(Self {
            matrix,
            canvas: Some(canvas),
            brightness: 100,
            physical_width,
            physical_height,
            rotation: (cfg.rotation % 4) as u8,
            buffer: RgbImage::new(physical_width, physical_height),
            scaled_buffer: vec![0u8; scaled_len],
            dirty_bounds: DirtyBounds::new(physical_height),
            pipeline,
        })
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl IDrawingSurface for CanvasBufferedSurface {
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

    fn physical_width(&self) -> u32 {
        self.physical_width
    }

    fn physical_height(&self) -> u32 {
        self.physical_height
    }

    fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
        self.dirty_bounds.mark_all(self.physical_height);
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
            self.dirty_bounds.mark(phys_y as u32);
        }
    }

    fn clear(&mut self) {
        self.buffer.as_flat_samples_mut().as_mut_slice().fill(0);
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn present(&mut self) {
        if !self.dirty_bounds.dirty {
            // O(1) early exit: zero memory bus transfers when frame has not changed
            return;
        }

        if let Some(ref canvas) = self.canvas {
            let handle = unsafe {
                *(canvas as *const rpi_led_matrix::LedCanvas as *const *mut std::ffi::c_void)
            };
            if !handle.is_null() {
                let b_scale = self.brightness as u16;
                match self.pipeline {
                    RenderPipeline::CanvasSingle | RenderPipeline::Auto => {
                        // Wi-Fi Protection Mode: push only the dirty row span to save bus bandwidth
                        let min_y = self.dirty_bounds.min_y;
                        let max_y = self.dirty_bounds.max_y;
                        let row_count = (max_y - min_y + 1) as usize;
                        let stride = (self.physical_width * 3) as usize;
                        let offset_bytes = min_y as usize * stride;
                        let total_bytes = row_count * stride;

                        let raw_slice =
                            &self.buffer.as_raw()[offset_bytes..offset_bytes + total_bytes];
                        let src_ptr = if b_scale < 100 {
                            let dst = &mut self.scaled_buffer[..total_bytes];
                            for (d, &s) in dst.iter_mut().zip(raw_slice.iter()) {
                                *d = ((s as u16 * b_scale) / 100) as u8;
                            }
                            dst.as_ptr()
                        } else {
                            raw_slice.as_ptr()
                        };

                        unsafe {
                            set_image(
                                handle,
                                0,
                                min_y as std::ffi::c_int,
                                src_ptr,
                                total_bytes,
                                self.physical_width as std::ffi::c_int,
                                row_count as std::ffi::c_int,
                                0,
                            );
                        }
                    }
                    _ => {
                        // Double-buffered canvas mode: full frame commit
                        let raw_slice = self.buffer.as_raw();
                        let total_bytes = raw_slice.len();
                        let src_ptr = if b_scale < 100 {
                            let dst = &mut self.scaled_buffer[..total_bytes];
                            for (d, &s) in dst.iter_mut().zip(raw_slice.iter()) {
                                *d = ((s as u16 * b_scale) / 100) as u8;
                            }
                            dst.as_ptr()
                        } else {
                            raw_slice.as_ptr()
                        };

                        unsafe {
                            set_image(
                                handle,
                                0,
                                0,
                                src_ptr,
                                total_bytes,
                                self.physical_width as std::ffi::c_int,
                                self.physical_height as std::ffi::c_int,
                                0,
                            );
                        }
                    }
                }
            }
        }

        if self.pipeline == RenderPipeline::CanvasDouble {
            let canvas = self.canvas.take().unwrap();
            self.canvas = Some(self.matrix.swap(canvas));
        }

        self.dirty_bounds.reset(self.physical_height);
    }

    fn set_brightness(&mut self, brightness: u8) {
        let new_b = brightness.min(100);
        if self.brightness == new_b {
            return;
        }
        self.brightness = new_b;
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn mark_dirty(&mut self) {
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn is_dirty(&self) -> bool {
        self.dirty_bounds.dirty
    }

    fn acquire_canvas(&mut self) -> Option<CanvasView<'_>> {
        let w = self.physical_width;
        let h = self.physical_height;
        let stride = (w * 3) as usize;
        Some(CanvasView {
            data: self.buffer.as_flat_samples_mut().samples,
            width: w,
            height: h,
            stride_bytes: stride,
        })
    }

    fn release_canvas(&mut self) {
        self.dirty_bounds.mark_all(self.physical_height);
    }

    fn has_canvas(&self) -> bool {
        true
    }
}

/// Direct Surface rendering directly to hardware LedCanvas.
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
pub struct DirectSurface {
    matrix: LedMatrix,
    canvas: Option<LedCanvas>,
    brightness: u8,
    physical_width: u32,
    physical_height: u32,
    rotation: u8,
    pipeline: RenderPipeline,
    dirty: bool,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Send for DirectSurface {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
unsafe impl Sync for DirectSurface {}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl DirectSurface {
    pub fn new(cfg: &MatrixConfig, pipeline: RenderPipeline) -> Result<Self, String> {
        let double_buffered = pipeline == RenderPipeline::DirectDouble;
        let (matrix, canvas, w, h) = init_led_matrix(cfg, double_buffered)?;
        Ok(Self {
            matrix,
            canvas: Some(canvas),
            brightness: 100,
            physical_width: w as u32,
            physical_height: h as u32,
            rotation: (cfg.rotation % 4) as u8,
            pipeline,
            dirty: true,
        })
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
impl IDrawingSurface for DirectSurface {
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

    fn physical_width(&self) -> u32 {
        self.physical_width
    }

    fn physical_height(&self) -> u32 {
        self.physical_height
    }

    fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
        self.dirty = true;
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
            if let Some(ref mut canvas) = self.canvas {
                let b_scale = self.brightness as u16;
                let color = rpi_led_matrix::LedColor {
                    red: ((red as u16 * b_scale) / 100) as u8,
                    green: ((green as u16 * b_scale) / 100) as u8,
                    blue: ((blue as u16 * b_scale) / 100) as u8,
                };
                canvas.set(phys_x, phys_y, &color);
                self.dirty = true;
            }
        }
    }

    fn clear(&mut self) {
        if let Some(ref mut canvas) = self.canvas {
            canvas.clear();
            self.dirty = true;
        }
    }

    fn present(&mut self) {
        if self.pipeline == RenderPipeline::DirectDouble {
            let canvas = self.canvas.take().unwrap();
            self.canvas = Some(self.matrix.swap(canvas));
        }
        self.dirty = false;
    }

    fn set_brightness(&mut self, brightness: u8) {
        let new_b = brightness.min(100);
        if self.brightness == new_b {
            return;
        }
        self.brightness = new_b;
        self.dirty = true;
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
fn init_led_matrix(
    cfg: &MatrixConfig,
    double_buffered: bool,
) -> Result<(LedMatrix, LedCanvas, usize, usize), String> {
    let mut options = LedMatrixOptions::new();
    options.set_rows(cfg.height);
    options.set_cols(cfg.width);
    options.set_chain_length(cfg.chain_length);
    options.set_parallel(cfg.parallel.clamp(1, 3));
    options.set_multiplexing(cfg.multiplexing);
    options.set_row_addr_type(cfg.row_address_mode);
    options.set_scan_mode(cfg.scan_mode);
    options.set_brightness(100).unwrap_or(());
    if !cfg.mapping.is_empty() {
        options.set_hardware_mapping(&cfg.mapping);
    }
    if !cfg.rgb_sequence.is_empty() {
        options.set_led_rgb_sequence(&cfg.rgb_sequence);
    }
    let _ = options.set_pwm_bits(cfg.pwm_bits as u8);
    options.set_pwm_lsb_nanoseconds(cfg.pwm_lsb_nanoseconds);
    options.set_hardware_pulsing(!cfg.disable_hardware_pulsing);

    let chip_candidate = if !cfg.panel_type.is_empty() {
        &cfg.panel_type
    } else {
        &cfg.driver_chip
    };
    if !chip_candidate.is_empty() {
        let chip_lower = chip_candidate.to_lowercase();
        if chip_lower == "fm6126a" || chip_lower == "fm6127" {
            options.set_panel_type(chip_candidate);
        }
    }

    options.set_limit_refresh(cfg.limit_refresh_rate_hz);
    options.set_refresh_rate(false);

    std::env::set_var("SUDO_UID", "0");
    std::env::set_var("SUDO_GID", "0");

    let mut rt_options = LedRuntimeOptions::new();
    rt_options.set_gpio_slowdown(cfg.slowdown);
    rt_options.set_drop_privileges(false);

    let matrix = LedMatrix::new(Some(options), Some(rt_options))
        .map_err(|e| format!("Failed to init LED matrix: {:?}", e))?;
    let canvas = if double_buffered {
        matrix.offscreen_canvas()
    } else {
        matrix.canvas()
    };
    let (w, h) = canvas.canvas_size();

    Ok((matrix, canvas, w as usize, h as usize))
}

/// Factory creating the canonical IDrawingSurface according to configuration.
pub fn create_surface(cfg: &MatrixConfig) -> Result<Box<dyn IDrawingSurface>, String> {
    #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
    {
        let pipeline = if cfg.force_single_buffer {
            match cfg.render_pipeline {
                RenderPipeline::DirectSingle | RenderPipeline::DirectDouble => {
                    RenderPipeline::DirectSingle
                }
                _ => RenderPipeline::CanvasSingle,
            }
        } else {
            match cfg.render_pipeline {
                RenderPipeline::Auto => RenderPipeline::CanvasSingle,
                other => other,
            }
        };

        match pipeline {
            RenderPipeline::DirectSingle | RenderPipeline::DirectDouble => {
                let surf = DirectSurface::new(cfg, pipeline)?;
                Ok(Box::new(surf))
            }
            RenderPipeline::CanvasSingle | RenderPipeline::CanvasDouble | RenderPipeline::Auto => {
                let surf = CanvasBufferedSurface::new(cfg, pipeline)?;
                Ok(Box::new(surf))
            }
        }
    }

    #[cfg(not(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64"))))]
    {
        let w = cfg.width * cfg.chain_length;
        let h = cfg.height * cfg.parallel.clamp(1, 3);
        let mut mock = MockSurface::new(w, h);
        mock.set_rotation((cfg.rotation % 4) as u8);
        Ok(Box::new(mock))
    }
}
