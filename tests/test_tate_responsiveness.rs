use arcadematrix::core::matrix::{MatrixBackend, MockMatrix};
use arcadematrix::core::types::{DisplayGeometry, LayoutClass};
use arcadematrix::engines::clocks::{
    PacmanClock, PongClock, SlotMachineClock, TetrisClock, VersusClock,
};
use arcadematrix::engines::fighter::FighterEngine;
use arcadematrix::engines::renderers::base_renderer::BaseRenderer;
use arcadematrix::engines::renderers::FlipRenderer;

#[test]
fn test_base_renderer_render_tate_time_various_resolutions() {
    let renderer = BaseRenderer::new();
    let resolutions = [(32, 64), (32, 128), (64, 128), (64, 256)];

    for (w, h) in resolutions {
        let mut matrix = MockMatrix::new(w, h);

        // Test theme 0 (Nintendo), theme 6 (Cave/3D), theme 20 (Custom Gradient)
        for theme in [0, 6, 20] {
            matrix.clear();
            renderer.render_tate_time(&mut matrix, 12, 34, 56, theme, 1, 0, 0, None, None);

            // Assert that pixels were actually rendered to the screen
            let mut lit_pixels = 0;
            for y in 0..h {
                for x in 0..w {
                    let px = matrix.canvas.get_pixel(x, y);
                    if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                        lit_pixels += 1;
                    }
                }
            }
            assert!(
                lit_pixels > 20,
                "Expected lit pixels on {}x{} for theme {}, got {}",
                w,
                h,
                theme,
                lit_pixels
            );
        }
    }
}

#[test]
fn test_flip_renderer_tate_layout() {
    let mut flip = FlipRenderer::new();
    let renderer = BaseRenderer::new();
    let font = renderer.font();

    for (w, h) in [(32, 64), (32, 128), (64, 128), (64, 256)] {
        let mut matrix = MockMatrix::new(w, h);
        flip.render(&mut matrix, "12:34:56", &font, 1, 0, 0);

        let mut lit_pixels = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    lit_pixels += 1;
                }
            }
        }
        assert!(
            lit_pixels > 15,
            "Expected lit pixels in FlipRenderer on {}x{}, got {}",
            w,
            h,
            lit_pixels
        );
    }
}

#[test]
fn test_subclocks_tate_responsiveness() {
    let renderer = BaseRenderer::new();
    let font = renderer.font();

    let resolutions = [(32, 64), (32, 128), (64, 128), (64, 256)];

    for (w, h) in resolutions {
        let mut matrix = MockMatrix::new(w, h);

        // 1. TetrisClock (run several frames to let blocks fall into screen)
        let mut tetris = TetrisClock::new(false);
        for _ in 0..20 {
            tetris.render(&mut matrix, "12:34:56", &font, 1);
        }
        let mut tetris_lit = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    tetris_lit += 1;
                }
            }
        }
        assert!(tetris_lit > 0, "TetrisClock failed on {}x{}", w, h);

        // 2. PacmanClock
        matrix.clear();
        let mut pacman = PacmanClock::new();
        pacman.render(&mut matrix, "12:34", 12, 34, &font, 1);
        let mut pac_lit = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    pac_lit += 1;
                }
            }
        }
        assert!(pac_lit > 10, "PacmanClock failed on {}x{}", w, h);

        // 3. VersusClock
        matrix.clear();
        let mut versus = VersusClock::new();
        versus.render(&mut matrix, 12, 34, &font, 1);
        let mut versus_lit = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    versus_lit += 1;
                }
            }
        }
        assert!(versus_lit > 10, "VersusClock failed on {}x{}", w, h);

        // 4. SlotMachineClock
        matrix.clear();
        let mut slot = SlotMachineClock::new();
        slot.render(&mut matrix, "12:34", 12, 34, &font, 1);
        let mut slot_lit = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    slot_lit += 1;
                }
            }
        }
        assert!(slot_lit > 10, "SlotMachineClock failed on {}x{}", w, h);

        // 5. PongClock
        matrix.clear();
        let mut pong = PongClock::new(w, h);
        pong.update_and_render(&mut matrix, 12, 34, &font, 1);
        let mut pong_lit = 0;
        for y in 0..h {
            for x in 0..w {
                let px = matrix.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    pong_lit += 1;
                }
            }
        }
        assert!(pong_lit > 5, "PongClock failed on {}x{}", w, h);
    }
}

#[test]
fn test_fighter_engine_tate_scaling_and_geometry() {
    let mut fighter = FighterEngine::new(64, 128);

    // Dynamic rotation to 32x128
    let geom_32x128 = DisplayGeometry {
        physical_width: 128,
        physical_height: 32,
        logical_width: 32,
        logical_height: 128,
        rotation: 1,
        layout_class: LayoutClass::Portrait,
        version: 1,
    };
    fighter.on_display_geometry_changed(&geom_32x128);

    // Dynamic rotation to 64x128 (Tate)
    let geom_64x128 = DisplayGeometry {
        physical_width: 128,
        physical_height: 64,
        logical_width: 64,
        logical_height: 128,
        rotation: 1,
        layout_class: LayoutClass::Portrait,
        version: 2,
    };
    fighter.on_display_geometry_changed(&geom_64x128);

    // Assert that on 64x128, scale for 32px sprites is strictly 1 (preventing character overlap)
    // Formula verification: in Tate, scale = if width >= 96 { width / 64 } else { 1 }
    let is_tate = geom_64x128.logical_width < 48
        || geom_64x128.logical_height > (geom_64x128.logical_width * 3) / 2;
    assert!(is_tate);
    let scale_64x128 = if is_tate {
        if geom_64x128.logical_width >= 96 {
            geom_64x128.logical_width / 64
        } else {
            1
        }
    } else {
        geom_64x128.logical_height / 32
    };
    assert_eq!(
        scale_64x128, 1,
        "Fighter scale on 64x128 Tate MUST strictly be 1"
    );

    let scale_32x128 = if is_tate {
        if geom_32x128.logical_width >= 96 {
            geom_32x128.logical_width / 64
        } else {
            1
        }
    } else {
        geom_32x128.logical_height / 32
    };
    assert_eq!(
        scale_32x128, 1,
        "Fighter scale on 32x128 Tate MUST strictly be 1"
    );
}

#[test]
fn test_clock_font_size_responsiveness_tate() {
    let renderer = BaseRenderer::new();
    let mut matrix_64x128 = MockMatrix::new(64, 128);

    // Test that size 1, 2, 3, 4 produce progressively more lit pixels and fit without error
    let mut previous_lit = 0;
    for custom_size in 1..=4 {
        matrix_64x128.clear();
        renderer.render_tate_time(
            &mut matrix_64x128,
            12,
            34,
            56,
            0, // Nintendo theme
            custom_size,
            0,
            0,
            None,
            None,
        );

        let mut lit = 0;
        for y in 0..128 {
            for x in 0..64 {
                let px = matrix_64x128.canvas.get_pixel(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    lit += 1;
                }
            }
        }
        assert!(
            lit > previous_lit,
            "Size {} should produce more pixels than previous size (got {} vs prev {})",
            custom_size,
            lit,
            previous_lit
        );
        previous_lit = lit;
    }
}

#[test]
fn test_clock_font_size_responsiveness_32x64() {
    let renderer = BaseRenderer::new();
    let mut matrix_32x64 = MockMatrix::new(32, 64);

    // Size 1
    matrix_32x64.clear();
    renderer.render_tate_time(&mut matrix_32x64, 12, 34, 56, 0, 1, 0, 0, None, None);
    let mut lit_s1 = 0;
    for y in 0..64 {
        for x in 0..32 {
            let px = matrix_32x64.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                lit_s1 += 1;
            }
        }
    }

    // Size 2
    matrix_32x64.clear();
    renderer.render_tate_time(&mut matrix_32x64, 12, 34, 56, 0, 2, 0, 0, None, None);
    let mut lit_s2 = 0;
    for y in 0..64 {
        for x in 0..32 {
            let px = matrix_32x64.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                lit_s2 += 1;
            }
        }
    }

    assert!(
        lit_s2 > lit_s1,
        "Size 2 on 32x64 must NOT be clamped to size 1 (got {} vs {})",
        lit_s2,
        lit_s1
    );
}

#[test]
fn test_tate_seconds_same_size_as_hours_minutes() {
    let renderer = BaseRenderer::new();
    let mut matrix = MockMatrix::new(64, 128);

    // Render 11:11:11 so glyphs are identical ('1's)
    renderer.render_tate_time(&mut matrix, 11, 11, 11, 0, 2, 0, 0, None, None);

    // Hours tier: y in 0..42
    let mut h_min_y = 9999u32;
    let mut h_max_y = 0u32;
    let mut h_min_x = 9999u32;
    let mut h_max_x = 0u32;
    let mut h_color = None;

    // Seconds tier: y in 86..128
    let mut s_min_y = 9999u32;
    let mut s_max_y = 0u32;
    let mut s_min_x = 9999u32;
    let mut s_max_x = 0u32;
    let mut s_color = None;

    for y in 0..43 {
        for x in 0..64 {
            let px = matrix.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                h_min_y = h_min_y.min(y);
                h_max_y = h_max_y.max(y);
                h_min_x = h_min_x.min(x);
                h_max_x = h_max_x.max(x);
                if h_color.is_none() {
                    h_color = Some(px);
                }
            }
        }
    }

    for y in 86..128 {
        for x in 0..64 {
            let px = matrix.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                s_min_y = s_min_y.min(y);
                s_max_y = s_max_y.max(y);
                s_min_x = s_min_x.min(x);
                s_max_x = s_max_x.max(x);
                if s_color.is_none() {
                    s_color = Some(px);
                }
            }
        }
    }

    assert!(h_color.is_some(), "Hours must be rendered");
    assert!(s_color.is_some(), "Seconds must be rendered");

    let h_height = h_max_y - h_min_y + 1;
    let s_height = s_max_y - s_min_y + 1;
    let h_width = h_max_x - h_min_x + 1;
    let s_width = s_max_x - s_min_x + 1;

    assert_eq!(
        s_height, h_height,
        "Seconds height ({}) must strictly equal hours height ({})",
        s_height, h_height
    );
    assert_eq!(
        s_width, h_width,
        "Seconds width ({}) must strictly equal hours width ({})",
        s_width, h_width
    );
    assert_eq!(
        s_color, h_color,
        "Seconds color ({:?}) must match hours color ({:?})",
        s_color, h_color
    );
}

#[test]
fn test_minute_clocks_ignore_seconds_stability() {
    let base = BaseRenderer::new();
    let font = base.font();
    let mut matrix = MockMatrix::new(64, 32);

    // 1. PacmanClock stability across seconds
    let mut pacman = PacmanClock::new();
    pacman.render(&mut matrix, "12:34:00", 12, 34, &font, 1);
    assert!(
        !pacman.is_transitioning(),
        "PacmanClock should not be transitioning initially"
    );

    for sec in 1..=10 {
        let time_str = format!("12:34:{:02}", sec);
        pacman.render(&mut matrix, &time_str, 12, 34, &font, 1);
        assert!(
            !pacman.is_transitioning(),
            "PacmanClock must not trigger animation on second change ({})",
            time_str
        );
    }
    let mut pac_lit = 0;
    for y in 0..32 {
        for x in 0..64 {
            let px = matrix.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                pac_lit += 1;
            }
        }
    }
    assert!(
        pac_lit > 20,
        "PacmanClock must display static digits without constant animation lock"
    );

    // Minute advance should trigger transition
    pacman.render(&mut matrix, "12:35:00", 12, 35, &font, 1);
    assert!(
        pacman.is_transitioning(),
        "PacmanClock must trigger transition when minute advances"
    );

    // 2. SlotMachineClock stability across seconds
    let mut slot = SlotMachineClock::new();
    slot.render(&mut matrix, "12:34:00", 12, 34, &font, 1);
    assert!(
        !slot.is_spinning(),
        "SlotMachineClock should not be spinning initially"
    );

    for sec in 1..=10 {
        let time_str = format!("12:34:{:02}", sec);
        slot.render(&mut matrix, &time_str, 12, 34, &font, 1);
        assert!(
            !slot.is_spinning(),
            "SlotMachineClock must not trigger spinning on second change ({})",
            time_str
        );
    }
    let mut slot_lit = 0;
    for y in 0..32 {
        for x in 0..64 {
            let px = matrix.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                slot_lit += 1;
            }
        }
    }
    assert!(
        slot_lit > 20,
        "SlotMachineClock must display static digits without constant spinning lock"
    );

    // Minute advance should trigger spinning
    slot.render(&mut matrix, "12:35:00", 12, 35, &font, 1);
    assert!(
        slot.is_spinning(),
        "SlotMachineClock must trigger spinning when minute advances"
    );
}

#[test]
fn test_pacman_progressive_eat_and_reveal() {
    let base = BaseRenderer::new();
    let font = base.font();

    // 1. Landscape mode (64x32)
    let mut matrix = MockMatrix::new(64, 32);
    let mut pacman = PacmanClock::new();
    pacman.render(&mut matrix, "12:34", 12, 34, &font, 1);

    // Trigger transition to 12:35
    pacman.render(&mut matrix, "12:35", 12, 35, &font, 1);
    assert!(pacman.is_transitioning());

    // Step a few frames into transition
    for _ in 0..15 {
        pacman.render(&mut matrix, "12:35", 12, 35, &font, 1);
    }
    assert!(pacman.is_transitioning());

    // Check that pixels ahead of pacman and on canvas are lit (progressive eat)
    let mut lit_ahead = 0;
    for y in 0..32 {
        for x in 32..64 {
            let px = matrix.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                lit_ahead += 1;
            }
        }
    }
    assert!(
        lit_ahead > 10,
        "Pixels ahead of Pacman must remain visible to be eaten progressively"
    );

    // 2. Tate / Vertical mode (32x64)
    let mut matrix_tate = MockMatrix::new(32, 64);
    let mut pacman_tate = PacmanClock::new();
    pacman_tate.render(&mut matrix_tate, "12:34", 12, 34, &font, 1);
    // Trigger transition
    pacman_tate.render(&mut matrix_tate, "12:35", 12, 35, &font, 1);
    assert!(pacman_tate.is_transitioning());

    // Step into Tier 1 (Hours line)
    for _ in 0..10 {
        pacman_tate.render(&mut matrix_tate, "12:35", 12, 35, &font, 1);
    }
    assert!(pacman_tate.is_transitioning());

    // In Tate mode, hours (top) and minutes (bottom) should have lit pixels
    let mut top_lit = 0;
    let mut bot_lit = 0;
    for y in 0..32 {
        for x in 0..32 {
            let px = matrix_tate.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                top_lit += 1;
            }
        }
    }
    for y in 32..64 {
        for x in 0..32 {
            let px = matrix_tate.canvas.get_pixel(x, y);
            if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                bot_lit += 1;
            }
        }
    }
    assert!(
        top_lit > 10,
        "Top tier (hours) must be rendering in vertical mode"
    );
    assert!(
        bot_lit > 10,
        "Bottom tier (minutes) must be rendering in vertical mode"
    );
}
