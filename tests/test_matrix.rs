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

#[test]
fn test_mock_matrix_rotations() {
    let mut matrix = MockMatrix::new(64, 32);

    // Rotation 0 (0°)
    matrix.set_rotation(0);
    assert_eq!(matrix.width(), 64);
    assert_eq!(matrix.height(), 32);
    matrix.clear();
    matrix.set_pixel(10, 5, 255, 100, 50);
    let px0 = matrix.canvas.get_pixel(10, 5);
    assert_eq!(px0[0], 255);

    // Rotation 1 (90° CW) -> logical 32x64
    matrix.set_rotation(1);
    assert_eq!(matrix.width(), 32);
    assert_eq!(matrix.height(), 64);
    matrix.clear();
    // logical (x=5, y=10) -> phys (x = 64 - 1 - 10 = 53, y = 5)
    matrix.set_pixel(5, 10, 200, 50, 100);
    let px1 = matrix.canvas.get_pixel(53, 5);
    assert_eq!(px1[0], 200);

    // Rotation 2 (180°) -> logical 64x32
    matrix.set_rotation(2);
    assert_eq!(matrix.width(), 64);
    assert_eq!(matrix.height(), 32);
    matrix.clear();
    // logical (x=10, y=5) -> phys (x = 64 - 1 - 10 = 53, y = 32 - 1 - 5 = 26)
    matrix.set_pixel(10, 5, 100, 255, 50);
    let px2 = matrix.canvas.get_pixel(53, 26);
    assert_eq!(px2[0], 100);

    // Rotation 3 (270° CCW) -> logical 32x64
    matrix.set_rotation(3);
    assert_eq!(matrix.width(), 32);
    assert_eq!(matrix.height(), 64);
    matrix.clear();
    // logical (x=5, y=10) -> phys (x = 10, y = 32 - 1 - 5 = 26)
    matrix.set_pixel(5, 10, 50, 100, 255);
    let px3 = matrix.canvas.get_pixel(10, 26);
    assert_eq!(px3[0], 50);
}
