use crate::core::matrix::MatrixBackend;
use image::RgbImage;

pub struct MarqueeEngine;

impl MarqueeEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, matrix: &mut dyn MatrixBackend, image: &RgbImage) {
        let mw = matrix.width() as u32;
        let mh = matrix.height() as u32;
        let iw = image.width();
        let ih = image.height();

        if iw == 0 || ih == 0 {
            return;
        }

        let scale_x = mw / iw;
        let scale_y = mh / ih;
        let scale = scale_x.min(scale_y);

        if scale > 1 {
            let new_w = iw * scale;
            let new_h = ih * scale;
            let scaled =
                image::imageops::resize(image, new_w, new_h, image::imageops::FilterType::Nearest);
            let offset_x = (mw - new_w) / 2;
            let offset_y = (mh - new_h) / 2;
            matrix.draw_image(&scaled, offset_x as i32, offset_y as i32);
        } else {
            let offset_x = if mw > iw { (mw - iw) / 2 } else { 0 };
            let offset_y = if mh > ih { (mh - ih) / 2 } else { 0 };
            matrix.draw_image(image, offset_x as i32, offset_y as i32);
        }
    }
}
