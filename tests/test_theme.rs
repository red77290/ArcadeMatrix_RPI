use arcadematrix::core::theme::{
    generate_gradient_mask, get_theme_info, interpolate_color, parse_hex_color,
};

#[test]
fn test_all_themes_info() {
    for id in 0..30 {
        let info = get_theme_info(id);
        assert_eq!(info.id, id);
        assert!(!info.name.is_empty());
    }

    assert_eq!(get_theme_info(0).name, "Nintendo");
    assert_eq!(get_theme_info(18).name, "Cyberpunk");
    assert_eq!(get_theme_info(21).name, "True Matrix");
    assert_eq!(get_theme_info(22).name, "Pong Clock");
    assert_eq!(get_theme_info(26).name, "Pac-Man Clock");
    assert_eq!(get_theme_info(27).name, "Versus Health Bar");
}

#[test]
fn test_parse_hex_color() {
    assert_eq!(parse_hex_color("#ff0000"), (255, 0, 0));
    assert_eq!(parse_hex_color("00ff00"), (0, 255, 0));
    assert_eq!(parse_hex_color("#0000ff"), (0, 0, 255));
    assert_eq!(parse_hex_color("invalid"), (255, 255, 255));
}

#[test]
fn test_color_interpolation() {
    let c1 = (0, 0, 0);
    let c2 = (100, 200, 50);

    assert_eq!(interpolate_color(c1, c2, 0.0), (0, 0, 0));
    assert_eq!(interpolate_color(c1, c2, 1.0), (100, 200, 50));
    assert_eq!(interpolate_color(c1, c2, 0.5), (50, 100, 25));
}

#[test]
fn test_gradient_mask_generation() {
    let mask = generate_gradient_mask(10, 10, (255, 0, 0), (0, 0, 255));
    assert_eq!(mask.width(), 10);
    assert_eq!(mask.height(), 10);
}
