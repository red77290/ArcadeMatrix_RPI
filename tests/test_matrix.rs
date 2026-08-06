use arcadematrix::core::matrix::{MatrixBackend, MockMatrix};
use image::{Rgb, RgbImage};

#[test]
fn test_mock_matrix_dimensions_and_pixels() {
    let mut matrix = MockMatrix::new(64, 32);
    assert_eq!(matrix.width(), 64);
    assert_eq!(matrix.height(), 32);

    matrix.set_pixel(10, 15, 255, 0, 128);
    let px = matrix.canvas.get_pixel(10, 15);
    assert_eq!(px[0], 255);
    assert_eq!(px[1], 0);
    assert_eq!(px[2], 128);

    matrix.clear();
    let cleared_px = matrix.canvas.get_pixel(10, 15);
    assert_eq!(cleared_px[0], 0);
    assert_eq!(cleared_px[1], 0);
    assert_eq!(cleared_px[2], 0);
}

#[test]
fn test_mock_matrix_draw_image() {
    let mut matrix = MockMatrix::new(64, 32);
    let mut img = RgbImage::new(10, 10);
    img.put_pixel(0, 0, Rgb([100, 200, 50]));

    matrix.draw_image(&img, 5, 5);
    let px = matrix.canvas.get_pixel(5, 5);
    assert_eq!(px[0], 100);
    assert_eq!(px[1], 200);
    assert_eq!(px[2], 50);
}
