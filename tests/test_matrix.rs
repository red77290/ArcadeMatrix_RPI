use arcadematrix::core::matrix::{IDrawingSurface, MockSurface};
use image::{Rgb, RgbImage};

#[test]
fn test_mock_matrix_dimensions_and_pixels() {
    let mut matrix = MockSurface::new(64, 32);
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
    let mut matrix = MockSurface::new(64, 32);
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
    let mut matrix = MockSurface::new(64, 32);

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

fn phys_pixel(surface: &mut dyn IDrawingSurface, x: u32, y: u32) -> [u8; 3] {
    let cv = surface
        .unscaled()
        .acquire_canvas()
        .expect("MockSurface always provides CanvasView");
    let idx = (y as usize) * cv.stride_bytes + (x as usize) * 3;
    [cv.data[idx], cv.data[idx + 1], cv.data[idx + 2]]
}

#[test]
fn test_ui_scale_auto_preserves_strict_1x_across_all_standard_and_tate_resolutions() {
    use arcadematrix::core::config::{MatrixConfig, UiScale};
    use arcadematrix::core::matrix::{normalize_rotation_code, ScaledDrawingSurface};
    use arcadematrix::core::orientation::OrientationManager;

    let standard_and_tate = [
        (64, 32),
        (32, 64),
        (64, 64),
        (128, 32),
        (32, 128),
        (128, 64),
        (64, 128),
        (256, 64),
        (64, 256),
        (128, 128),
    ];

    for &(w, h) in &standard_and_tate {
        assert_eq!(
            UiScale::Auto.resolve(w, h),
            1,
            "UiScale::Auto MUST remain strictly 1x for {}x{}",
            w,
            h
        );

        let mut cfg = MatrixConfig::default();
        cfg.width = w;
        cfg.height = h;
        cfg.chain_length = 1;
        cfg.parallel = 1;
        cfg.ui_scale = UiScale::Auto;

        assert_eq!(cfg.resolve_ui_scale(), 1);
        assert_eq!(cfg.effective_native_dimensions(), (w, h));
        assert_eq!(cfg.effective_logical_dimensions(), (w, h));

        let om = OrientationManager::new_with_scale(w, h, 0, cfg.resolve_ui_scale());
        assert_eq!(om.geometry().width(), w);
        assert_eq!(om.geometry().height(), h);
        assert_eq!(om.geometry().native_width(), w);
        assert_eq!(om.geometry().native_height(), h);

        // Compare ScaledDrawingSurface(scale=1) pixel-for-pixel against bare MockSurface
        let mut bare = MockSurface::new(w, h);
        let mut scaled = ScaledDrawingSurface::new(Box::new(MockSurface::new(w, h)), 1);

        assert_eq!(scaled.ui_scale(), 1);
        assert_eq!(scaled.width(), w);
        assert_eq!(scaled.height(), h);
        assert_eq!(scaled.native_width(), w);
        assert_eq!(scaled.native_height(), h);
        assert!(
            scaled.has_canvas(),
            "1x ScaledDrawingSurface must expose underlying CanvasView for {}x{}",
            w,
            h
        );
        {
            let cv = scaled
                .acquire_canvas()
                .expect("1x ScaledDrawingSurface must provide CanvasView");
            assert_eq!(cv.width, w);
            assert_eq!(cv.height, h);
        }
        bare.present();
        scaled.present();
        assert!(!scaled.is_dirty());
        assert_eq!(scaled.dirty_row_span(), None);

        let probes = [
            (0, 0, 255, 10, 20),
            ((w as i32) - 1, 0, 30, 240, 50),
            (0, (h as i32) - 1, 60, 70, 250),
            ((w as i32) - 1, (h as i32) - 1, 200, 180, 90),
            ((w as i32) / 2, (h as i32) / 2, 123, 222, 45),
        ];
        for &(px, py, r, g, b) in &probes {
            bare.set_pixel(px, py, r, g, b);
            scaled.set_pixel(px, py, r, g, b);
        }
        bare.fill_rect(3, 2, 5, 4, 17, 88, 199);
        scaled.fill_rect(3, 2, 5, 4, 17, 88, 199);
        assert!(scaled.is_dirty());
        assert_eq!(scaled.dirty_row_span(), bare.dirty_row_span());

        let scaled_bytes = scaled.acquire_canvas().unwrap().data.to_vec();
        assert_eq!(
            scaled_bytes.as_slice(),
            bare.canvas.as_raw().as_slice(),
            "1x ScaledDrawingSurface altered pixels on {}x{}",
            w,
            h
        );
    }

    // Also verify rotated TATE configurations (rotation = 90 and 270) on 128x32 and 256x64
    for &(phys_w, phys_h, rot_deg, exp_w, exp_h) in &[
        (128u32, 32u32, 90u32, 32u32, 128u32),
        (128u32, 32u32, 270u32, 32u32, 128u32),
        (256u32, 64u32, 90u32, 64u32, 256u32),
        (256u32, 64u32, 270u32, 64u32, 256u32),
    ] {
        let mut cfg = MatrixConfig::default();
        cfg.width = phys_w;
        cfg.height = phys_h;
        cfg.chain_length = 1;
        cfg.parallel = 1;
        cfg.rotation = rot_deg;
        cfg.ui_scale = UiScale::Auto;

        assert_eq!(cfg.resolve_ui_scale(), 1);
        assert_eq!(cfg.effective_native_dimensions(), (exp_w, exp_h));
        assert_eq!(cfg.effective_logical_dimensions(), (exp_w, exp_h));

        let mut mock = MockSurface::new(phys_w, phys_h);
        mock.set_rotation(normalize_rotation_code(rot_deg));
        let mut surf = ScaledDrawingSurface::new(Box::new(mock), cfg.resolve_ui_scale());
        assert_eq!(surf.ui_scale(), 1);
        assert_eq!(surf.width(), exp_w);
        assert_eq!(surf.height(), exp_h);
        surf.present();
        surf.set_pixel(4, 9, 210, 110, 55);
        assert!(surf.is_dirty());
        surf.present();
        assert!(!surf.is_dirty());
    }
}

#[test]
fn test_ui_scale_giant_led_walls_nearest_neighbor_and_dirty_bounds() {
    use arcadematrix::core::config::UiScale;
    use arcadematrix::core::matrix::ScaledDrawingSurface;

    // Auto scaling thresholds: triggers ONLY when min >= 128 && max >= 192
    assert_eq!(UiScale::Auto.resolve(256, 192), 2);
    assert_eq!(UiScale::Auto.resolve(192, 256), 2);
    assert_eq!(UiScale::Auto.resolve(384, 288), 3);
    assert_eq!(UiScale::Auto.resolve(288, 384), 3);
    assert_eq!(UiScale::Auto.resolve(512, 384), 4);
    assert_eq!(UiScale::Auto.resolve(384, 512), 4);

    // 256x192 at Auto (2x -> logical 128x96)
    let mut surf = ScaledDrawingSurface::new(
        Box::new(MockSurface::new(256, 192)),
        UiScale::Auto.resolve(256, 192),
    );
    assert_eq!(surf.ui_scale(), 2);
    assert_eq!(surf.width(), 128);
    assert_eq!(surf.height(), 96);
    assert_eq!(surf.native_width(), 256);
    assert_eq!(surf.native_height(), 192);
    assert!(!surf.has_canvas());

    surf.present();
    assert!(!surf.is_dirty());
    assert_eq!(surf.dirty_row_span(), None);

    // Single logical pixel (10, 7) -> exact 2x2 block at physical [20..22, 14..16]
    surf.set_pixel(10, 7, 250, 120, 40);
    assert!(surf.is_dirty());
    assert_eq!(surf.dirty_row_span(), Some((14, 15)));

    for py in 14..16 {
        for px in 20..22 {
            assert_eq!(phys_pixel(&mut surf, px, py), [250, 120, 40]);
        }
    }
    assert_eq!(phys_pixel(&mut surf, 19, 14), [0, 0, 0]);
    assert_eq!(phys_pixel(&mut surf, 22, 15), [0, 0, 0]);
    assert_eq!(phys_pixel(&mut surf, 20, 13), [0, 0, 0]);
    assert_eq!(phys_pixel(&mut surf, 20, 16), [0, 0, 0]);

    surf.present();
    assert!(!surf.is_dirty());
    assert_eq!(surf.dirty_row_span(), None);

    // Manual 3x on 256x192 -> logical 85x64
    surf.set_ui_scale(3);
    assert_eq!(surf.ui_scale(), 3);
    assert_eq!(surf.width(), 85);
    assert_eq!(surf.height(), 64);

    // 512x384 at Auto (4x -> logical 128x96)
    let mut wall_4x = ScaledDrawingSurface::new(
        Box::new(MockSurface::new(512, 384)),
        UiScale::Auto.resolve(512, 384),
    );
    assert_eq!(wall_4x.ui_scale(), 4);
    assert_eq!(wall_4x.width(), 128);
    assert_eq!(wall_4x.height(), 96);
    wall_4x.present();
    wall_4x.fill_rect(2, 3, 2, 1, 12, 234, 56);
    assert_eq!(wall_4x.dirty_row_span(), Some((12, 15)));
    for py in 12..16 {
        for px in 8..16 {
            assert_eq!(phys_pixel(&mut wall_4x, px, py), [12, 234, 56]);
        }
    }
    assert_eq!(phys_pixel(&mut wall_4x, 7, 12), [0, 0, 0]);
    assert_eq!(phys_pixel(&mut wall_4x, 16, 15), [0, 0, 0]);
}

#[test]
fn test_pixel_mapper_folding_and_config_serde() {
    use arcadematrix::core::config::{MatrixConfig, UiScale};

    // 1. 8 chained 64x32 panels with U-mapper (parallel=1) -> 256x64 physical -> stays 1x!
    let cfg_256x64: MatrixConfig = serde_json::from_str(
        r#"{
            "width": 64,
            "height": 32,
            "chain_length": 8,
            "parallel": 1,
            "pixel_mapper": "U-mapper",
            "ui_scale": "auto"
        }"#,
    )
    .expect("valid MatrixConfig JSON");
    assert_eq!(cfg_256x64.effective_physical_dimensions(), (256, 64));
    assert_eq!(cfg_256x64.resolve_ui_scale(), 1);
    assert_eq!(cfg_256x64.effective_logical_dimensions(), (256, 64));

    // 2. 8x3 = 24 panels of 64x32 with U-mapper (chain=8, parallel=3) -> 256x192 -> Auto 2x (128x96)
    let cfg_256x192: MatrixConfig = serde_json::from_str(
        r#"{
            "width": 64,
            "height": 32,
            "chain_length": 8,
            "parallel": 3,
            "pixel_mapper": "U-mapper",
            "ui_scale": "auto"
        }"#,
    )
    .expect("valid MatrixConfig JSON");
    assert_eq!(cfg_256x192.effective_physical_dimensions(), (256, 192));
    assert_eq!(cfg_256x192.resolve_ui_scale(), 2);
    assert_eq!(cfg_256x192.effective_logical_dimensions(), (128, 96));

    // 3. V-mapper on 4 chained 64x32 panels (parallel=1) -> 64x128 TATE -> stays 1x!
    let cfg_vmapper: MatrixConfig = serde_json::from_str(
        r#"{
            "width": 64,
            "height": 32,
            "chain_length": 4,
            "parallel": 1,
            "pixel_mapper": "V-mapper"
        }"#,
    )
    .expect("valid MatrixConfig JSON");
    assert_eq!(cfg_vmapper.effective_physical_dimensions(), (64, 128));
    assert_eq!(cfg_vmapper.resolve_ui_scale(), 1);
    assert_eq!(cfg_vmapper.effective_logical_dimensions(), (64, 128));

    // 4. Flexible UiScale deserialization ("2", "2x", integer 2, unknown -> Auto)
    let s_num: UiScale = serde_json::from_str("2").unwrap();
    let s_str: UiScale = serde_json::from_str(r#""2x""#).unwrap();
    let s_fallback: UiScale = serde_json::from_str(r#""invalid""#).unwrap();
    assert_eq!(s_num, UiScale::Scale2x);
    assert_eq!(s_str, UiScale::Scale2x);
    assert_eq!(s_fallback, UiScale::Auto);
    assert_eq!(serde_json::to_string(&s_num).unwrap(), r#""2""#);
}

#[test]
fn test_gif_and_showcase_stream_stay_native_1x_while_showcase_card_upscales() {
    use arcadematrix::core::config::Config;
    use arcadematrix::core::dmd_stream::PreloadedDmdStream;
    use arcadematrix::core::engine_contract::{Engine, EngineContext, HashConfig};
    use arcadematrix::core::matrix::ScaledDrawingSurface;
    use arcadematrix::engines::gif::GifEngine;
    use arcadematrix::engines::renderers::media_showcase::{
        MediaShowcaseRunner, PlaybackPhase, PreparedShowcaseItem, ShowcaseDomain,
    };
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba, RgbaImage};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let cfg_path = temp_dir.path().join("config.json");
    let _ = std::fs::copy("config.json", &cfg_path);
    let config = Config::new(&cfg_path);

    // Giant 256x192 LED wall with 2x UI scale (logical 128x96, native 256x192)
    let mut scaled_surface = ScaledDrawingSurface::new(Box::new(MockSurface::new(256, 192)), 2);
    assert_eq!(scaled_surface.width(), 128);
    assert_eq!(scaled_surface.height(), 96);
    assert_eq!(scaled_surface.native_width(), 256);
    assert_eq!(scaled_surface.native_height(), 192);

    // Part A: GifEngine MUST decode & render at native 1x full-screen (256x192)
    let gif_path = temp_dir.path().join("native_wall.gif");
    {
        let mut rgba = RgbaImage::from_pixel(256, 192, Rgba([0, 0, 0, 255]));
        // Put distinct fine-grained 1px neighbors at (0, 0) = Red and (1, 0) = Blue
        rgba.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        rgba.put_pixel(1, 0, Rgba([0, 0, 255, 255]));
        rgba.put_pixel(255, 191, Rgba([0, 255, 0, 255]));
        let file = std::fs::File::create(&gif_path).expect("create gif");
        let mut encoder = GifEncoder::new(file);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        let frame = Frame::from_parts(rgba, 0, 0, Delay::from_numer_denom_ms(100, 1));
        encoder.encode_frame(frame).unwrap();
    }

    let empty_map = HashMap::new();
    let hash_cfg = HashConfig { data: &empty_map };
    let mut gif_engine = GifEngine::new(64, 32);
    {
        let mut ctx = EngineContext {
            surface: &mut scaled_surface,
            config: &config,
        };
        gif_engine.initialize(&mut ctx, &hash_cfg).unwrap();
        assert!(gif_engine.load_gif(&gif_path));
        gif_engine.render(&mut ctx);
    }
    assert_eq!(phys_pixel(&mut scaled_surface, 0, 0), [255, 0, 0]);
    assert_eq!(
        phys_pixel(&mut scaled_surface, 1, 0),
        [0, 0, 255],
        "GifEngine must render at native 1x (1px precision), not 2x block-upscaled"
    );
    assert_eq!(phys_pixel(&mut scaled_surface, 255, 191), [0, 255, 0]);

    // Part B: MediaShowcaseRunner Phase 1 Card upscales (2x) while Phase 2 LiveStream stays native 1x!
    scaled_surface.clear();
    let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32);
    {
        let mut ctx = EngineContext {
            surface: &mut scaled_surface,
            config: &config,
        };
        let mut showcase_map = HashMap::new();
        showcase_map.insert("enabled".to_string(), "false".to_string());
        let sc_cfg = HashConfig {
            data: &showcase_map,
        };
        runner.initialize(&mut ctx, &sc_cfg);
    }
    assert_eq!(runner.width, 256);
    assert_eq!(runner.height, 192);
    assert_eq!(runner.card_width, 128);
    assert_eq!(runner.card_height, 96);
    assert!(
        runner.dmd_client_id().ends_with("_games_256x192"),
        "Showcase client_id must advertise native 256x192 stream resolution, got {}",
        runner.dmd_client_id()
    );

    // Prepare an item with a 4x96 poster (green at (0, 0)) + a 256x192 native stream
    let mut poster = RgbImage::new(4, 96);
    poster.put_pixel(0, 0, Rgb([0, 255, 0]));
    let mut raw_stream = vec![0u8; 256 * 192 * 2];
    // Native pixel (0, 0) = Pure Red RGB565 (0xF800), (1, 0) = Pure Blue RGB565 (0x001F)
    raw_stream[0..2].copy_from_slice(&0xF800u16.to_le_bytes());
    raw_stream[2..4].copy_from_slice(&0x001Fu16.to_le_bytes());
    let stream_arc = Arc::new(PreloadedDmdStream::from_raw_rgb565_le(
        256,
        192,
        1,
        Duration::from_millis(66),
        raw_stream,
    ));

    runner.turn_evaluated = true;
    runner.phase = PlaybackPhase::Card;
    runner.phase_started_at = Instant::now();
    runner.active_item = Some(PreparedShowcaseItem {
        id: "pkg_games_testwall".to_string(),
        title: "METAL SLUG".to_string(),
        year: "1996".to_string(),
        badge: "NEOGEO".to_string(),
        badge_rgb: (255, 80, 0),
        subtitle: "SNK".to_string(),
        rating: "9.5".to_string(),
        synopsis: "Run and gun classic.".to_string(),
        poster: Some(poster),
        has_stream: true,
        stream_duration_s: 4.0,
        stream_fps: 15,
        http_stream: Some(Arc::clone(&stream_arc)),
        http_frame_delay: Duration::from_millis(66),
    });

    // Render Phase 1 Card -> logical (0, 0) green pixel MUST expand to a 2x2 block at [0..2, 0..2]
    {
        let mut ctx = EngineContext {
            surface: &mut scaled_surface,
            config: &config,
        };
        runner.render(&mut ctx);
    }
    assert_eq!(phys_pixel(&mut scaled_surface, 0, 0), [0, 255, 0]);
    assert_eq!(
        phys_pixel(&mut scaled_surface, 1, 0),
        [0, 255, 0],
        "Phase 1 Intro Card must benefit from 2x Nearest-Neighbor upscaling on 256x192"
    );
    assert_eq!(phys_pixel(&mut scaled_surface, 0, 1), [0, 255, 0]);
    assert_eq!(phys_pixel(&mut scaled_surface, 1, 1), [0, 255, 0]);

    // Switch to Phase 2 LiveStream -> native 256x192 stream MUST render 1:1 without 2x expansion
    scaled_surface.clear();
    runner.phase = PlaybackPhase::LiveStream;
    runner.stream_player.start(Instant::now());
    {
        let mut ctx = EngineContext {
            surface: &mut scaled_surface,
            config: &config,
        };
        runner.render(&mut ctx);
    }
    assert_eq!(phys_pixel(&mut scaled_surface, 0, 0), [255, 0, 0]);
    assert_eq!(
        phys_pixel(&mut scaled_surface, 1, 0),
        [0, 0, 255],
        "Phase 2 LiveStream video/panoramas must render at native 1x full-screen on 256x192"
    );
}
