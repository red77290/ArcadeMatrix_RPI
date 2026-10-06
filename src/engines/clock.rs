use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::engines::clocks::*;
use crate::engines::renderers::*;
use chrono::Timelike;
use linkme::distributed_slice;

pub struct ClockEngine {
    base_renderer: BaseRenderer,
    cyberpunk: CyberpunkRenderer,
    flip: FlipRenderer,
    true_matrix: TrueMatrixRenderer,
    pong: PongClock,
    tetris: TetrisClock,
    tetris_gb: TetrisClock,
    word: WordClock,
    binary: BinaryClock,
    pacman: PacmanClock,
    ms_pacman: PacmanClock,
    castle: CastleClock,
    worldmap: WorldMapClock,
    pokedex: PokedexClock,
    mario: MarioClock,
    words: WordsClock,
    versus: VersusClock,
    slot_machine: SlotMachineClock,

    // Config states
    time_format: String,
    time_font: String,
    time_size: u32,
    time_theme: i32,
    timezone: String,
    clock_color_1: String,
    clock_color_2: String,
    clock_glow: i32,
    clock_glow_color: String,
    /// `clock_glow_color` parsed once, so render() does not re-read the string every frame.
    clock_glow_rgb: (u8, u8, u8),
    time_offset_x: i32,
    time_offset_y: i32,

    last_font: String,
    last_theme: i32,
    last_size: u32,
}

pub fn parse_tz(tz_str: &str) -> Option<chrono_tz::Tz> {
    let clean = tz_str.trim();
    if clean.is_empty() {
        return None;
    }
    // 1. Full IANA timezone database (600+ world timezones: Europe/Paris, America/New_York, Asia/Tokyo, etc.)
    if let Ok(tz) = clean.parse::<chrono_tz::Tz>() {
        return Some(tz);
    }

    // 2. POSIX string prefix & alias resolution for standard global regions
    let upper = clean.to_uppercase();
    if upper.starts_with("CET") || upper.starts_with("CEST") {
        return "Europe/Paris".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("WET") || upper.starts_with("WEST") || upper.starts_with("GMT0BST") {
        return "Europe/London".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("EET") || upper.starts_with("EEST") {
        return "Europe/Athens".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("MSK") {
        return "Europe/Moscow".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("EST5EDT") || upper.starts_with("EST") {
        return "America/New_York".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("CST6CDT") {
        return "America/Chicago".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("MST7MDT") || upper.starts_with("MST") {
        return "America/Denver".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("PST8PDT") || upper.starts_with("PST") {
        return "America/Los_Angeles".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("AKST") {
        return "America/Anchorage".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("HST") {
        return "Pacific/Honolulu".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("JST") {
        return "Asia/Tokyo".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("CST-8") || upper.starts_with("HKT") {
        return "Asia/Shanghai".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("SGT") {
        return "Asia/Singapore".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("IST") {
        return "Asia/Kolkata".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("AEST") || upper.starts_with("AEDT") {
        return "Australia/Sydney".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("ACST") {
        return "Australia/Adelaide".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("AWST") {
        return "Australia/Perth".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("NZST") || upper.starts_with("NZDT") {
        return "Pacific/Auckland".parse::<chrono_tz::Tz>().ok();
    }
    if upper.starts_with("BRT") {
        return "America/Sao_Paulo".parse::<chrono_tz::Tz>().ok();
    }

    // 3. Etc/GMT offset mapping (e.g. UTC+2, GMT-3, etc.)
    if upper.starts_with("UTC") || upper.starts_with("GMT") {
        let rest = upper
            .trim_start_matches("UTC")
            .trim_start_matches("GMT")
            .trim();
        if rest.is_empty() {
            return Some(chrono_tz::UTC);
        }
        if let Ok(offset) = rest.parse::<i32>() {
            let iana_name = if offset > 0 {
                format!("Etc/GMT-{}", offset)
            } else {
                format!("Etc/GMT+{}", -offset)
            };
            if let Ok(tz) = iana_name.parse::<chrono_tz::Tz>() {
                return Some(tz);
            }
        }
    }

    None
}

impl ClockEngine {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cyberpunk: CyberpunkRenderer::new(w, h),
            flip: FlipRenderer::new(),
            true_matrix: TrueMatrixRenderer::new(w, h),
            pong: PongClock::new(w, h),
            tetris: TetrisClock::new(false),
            tetris_gb: TetrisClock::new(true),
            word: WordClock::new(),
            binary: BinaryClock::new(),
            pacman: PacmanClock::new(),
            ms_pacman: PacmanClock::new_ms(),
            castle: CastleClock::new(),
            worldmap: WorldMapClock::new(),
            pokedex: PokedexClock::new(),
            mario: MarioClock::new(),
            words: WordsClock::new(),
            versus: VersusClock::new(),
            slot_machine: SlotMachineClock::new(),

            time_format: "%H:%M:%S".to_string(),
            time_font: "PressStart2P.ttf".to_string(),
            time_size: 2,
            time_theme: 0,
            timezone: "".to_string(),
            clock_color_1: "#ffffff".to_string(),
            clock_glow: 0,
            clock_glow_color: "#00ff41".to_string(),
            clock_glow_rgb: (0, 255, 65),
            clock_color_2: "#ffffff".to_string(),
            time_offset_x: 0,
            time_offset_y: 0,

            last_font: String::new(),
            last_theme: -1,
            last_size: 0,
        }
    }

    /// Reads every configurable field from the instance config. Supports both
    /// unprefixed (format, font, size, theme) and legacy prefixed (clock_format,
    /// clock_font, clock_size, clock_theme) keys.
    fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
        let h = hex.trim().trim_start_matches('#');
        if h.len() < 6 {
            return None;
        }
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some((r, g, b))
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        let fmt = config.get_string("format", "");
        self.time_format = if !fmt.is_empty() {
            fmt
        } else {
            config.get_string("clock_format", "%H:%M:%S")
        };

        self.clock_glow = config.get_int("clock_glow", 0) as i32;
        self.clock_glow_color = config.get_string("clock_glow_color", "#00ff41");
        self.clock_glow_rgb = Self::parse_hex(&self.clock_glow_color).unwrap_or((0, 255, 65));

        let font = config.get_string("font", "");
        let font = if !font.is_empty() {
            font
        } else {
            config.get_string("clock_font", "")
        };
        let font = if !font.is_empty() {
            font
        } else {
            config.get_string("clock_font_path", "")
        };
        self.time_font = if !font.is_empty() {
            font
        } else {
            config.get_string("font_path", "PressStart2P.ttf")
        };

        let size = config.get_int("size", 0);
        self.time_size = if size > 0 {
            size as u32
        } else {
            let cs = config.get_int("clock_size", 2);
            if cs > 0 {
                cs as u32
            } else {
                2
            }
        };

        let theme = config.get_int("theme", -1);
        self.time_theme = if theme >= 0 {
            theme
        } else {
            config.get_int("clock_theme", 0)
        };

        let tz = config.get_string("timezone", "");
        self.timezone = if !tz.is_empty() {
            tz
        } else {
            config.get_string("clock_timezone", "")
        };

        let c1 = config.get_string("color_1", "");
        self.clock_color_1 = if !c1.is_empty() {
            c1
        } else {
            config.get_string("clock_color_1", "#ffffff")
        };

        let c2 = config.get_string("color_2", "");
        self.clock_color_2 = if !c2.is_empty() {
            c2
        } else {
            config.get_string("clock_color_2", "#ffffff")
        };

        let ox = config.get_int("offset_x", 0);
        self.time_offset_x = if ox != 0 {
            ox
        } else {
            config.get_int("clock_offset_x", 0)
        };

        let oy = config.get_int("offset_y", 0);
        self.time_offset_y = if oy != 0 {
            oy
        } else {
            config.get_int("clock_offset_y", 0)
        };

        // The parade honours clock_speed and the offsets, as it does on the ESP32.
        let speed_pct = config.get_int("clock_speed", 100) as i32;
        // Only pass a colour the instance actually chose; unset lets the face use its own palette.
        let digit_color = if config.get_string("color_1", "").is_empty()
            && config.get_string("clock_color_1", "").is_empty()
        {
            None
        } else {
            parse_hex_color(&self.clock_color_1)
        };
        self.pacman.configure(
            speed_pct,
            self.time_offset_x,
            self.time_offset_y,
            digit_color,
        );
        self.ms_pacman.configure(
            speed_pct,
            self.time_offset_x,
            self.time_offset_y,
            digit_color,
        );
        self.tetris.configure(speed_pct);
        self.tetris_gb.configure(speed_pct);
        self.mario.configure(speed_pct);
    }
}

impl Engine for ClockEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {
        // Rotation and the end of a preemption (resume() defaults to activate())
        // both come through here: faces with their own state show the current
        // time at once instead of what they last showed.
        self.mario.on_activated();
    }
    fn deactivate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }

    fn is_realtime(&self) -> bool {
        crate::core::theme::is_realtime_theme(self.time_theme)
    }

    fn render(&mut self, context: &mut EngineContext) {
        // Faces draw through BaseRenderer, which reads this when it lays the outline down.
        BaseRenderer::set_glow(self.clock_glow.clamp(0, 2) as u8, self.clock_glow_rgb);
        let matrix = &mut *context.matrix;

        let (sys_tz, sys_24h) = {
            let sys = context.config.settings.read();
            (sys.system.timezone.clone(), sys.system.format_24h)
        };

        let tz_str = if !self.timezone.is_empty() && self.timezone != "system" {
            self.timezone.clone()
        } else {
            sys_tz
        };

        let now = if let Some(tz) = parse_tz(&tz_str) {
            chrono::Utc::now().with_timezone(&tz).naive_local()
        } else {
            chrono::Local::now().naive_local()
        };

        // If system format is 12h and format string uses %H, adapt dynamically to %I
        let mut base_format = self.time_format.clone();
        if !sys_24h {
            base_format = base_format.replace("%H", "%I");
        }

        // Full time string with seconds (for binary clock)
        let time_str_full = now.format(&base_format).to_string();

        // Short time string for display clocks
        let mut format_str = base_format.clone();
        if self.time_theme == 19 && !format_str.contains("%S") {
            format_str.push_str(":%S");
        }
        let time_str = now.format(&format_str).to_string();

        let hours = now.hour();
        let minutes = now.minute();
        let seconds = now.second();

        // Reload font from disk if the config font changes
        let mut reset_clocks = false;
        if self.time_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&self.time_font);
            self.last_font = self.time_font.clone();
            reset_clocks = true;
        }
        if self.time_theme != self.last_theme {
            tracing::info!(
                from = self.last_theme,
                to = self.time_theme,
                "clock theme change -> resetting sub-clocks"
            );
            self.last_theme = self.time_theme;
            reset_clocks = true;
        }
        if self.time_size != self.last_size {
            self.last_size = self.time_size;
            reset_clocks = true;
        }

        if reset_clocks {
            let w = matrix.width() as u32;
            let h = matrix.height() as u32;
            self.cyberpunk = CyberpunkRenderer::new(w, h);
            self.true_matrix = TrueMatrixRenderer::new(w, h);
            self.pong = PongClock::new(w, h);
            self.tetris = TetrisClock::new(false);
            self.tetris_gb = TetrisClock::new(true);
            self.word = WordClock::new();
            self.binary = BinaryClock::new();
            self.pacman = PacmanClock::new();
            self.versus = VersusClock::new();
            self.slot_machine = SlotMachineClock::new();
            self.flip.reset();
            matrix.clear(); // Clear artifact pixels from old layout
        }

        let font = self.base_renderer.font();
        let effective_size = self.time_size.max(1);

        let is_tate = matrix.width() < 48 || matrix.height() > (matrix.width() * 3) / 2;

        match self.time_theme {
            18 => {
                self.cyberpunk.render(matrix);
                if is_tate {
                    self.base_renderer.render_tate_time(
                        matrix,
                        hours,
                        minutes,
                        seconds,
                        18,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some((0, 255, 128)),
                        Some((0, 0, 0)),
                    );
                } else {
                    self.base_renderer.render_text(
                        matrix,
                        &time_str,
                        18,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some((0, 140, 0)),
                        Some((0, 0, 0)),
                    );
                }
            }
            21 => {
                self.true_matrix.render(matrix);
                if is_tate {
                    self.base_renderer.render_tate_time(
                        matrix,
                        hours,
                        minutes,
                        seconds,
                        21,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some((0, 255, 70)),
                        Some((0, 0, 0)),
                    );
                } else {
                    self.base_renderer.render_text(
                        matrix,
                        &time_str,
                        21,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some((0, 140, 0)),
                        Some((0, 0, 0)),
                    );
                }
            }
            19 => self.flip.render(
                matrix,
                &time_str,
                &font,
                effective_size,
                self.time_offset_x,
                self.time_offset_y,
            ),
            20 => {
                // Custom Gradient
                let color1 = parse_hex_color(&self.clock_color_1).unwrap_or((0, 255, 255));
                let color2 = parse_hex_color(&self.clock_color_2).unwrap_or((255, 0, 255));
                if is_tate {
                    self.base_renderer.render_tate_time(
                        matrix,
                        hours,
                        minutes,
                        seconds,
                        20,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some(color1),
                        Some(color2),
                    );
                } else {
                    self.base_renderer.render_text(
                        matrix,
                        &time_str,
                        20,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        Some(color1),
                        Some(color2),
                    );
                }
            }
            22 => self
                .pong
                .update_and_render(matrix, hours, minutes, &font, effective_size),
            23 => self.tetris.render(matrix, &time_str, &font, effective_size),
            24 => self.word.render(
                matrix,
                hours,
                minutes,
                &font,
                effective_size,
                &context.config.settings.read().system.lang,
            ),
            25 => self
                .binary
                .render(matrix, hours, minutes, seconds, &font, effective_size),
            26 => self
                .pacman
                .render(matrix, &time_str, hours, minutes, &font, effective_size),
            30 => self
                .mario
                .render(matrix, hours, minutes, &font, effective_size),
            31 => self.castle.render(matrix, hours, minutes),
            32 => self
                .pokedex
                .render(matrix, hours, minutes, seconds, &font, effective_size),
            33 => self
                .worldmap
                .render(matrix, hours, minutes, &font, effective_size),
            34 => self
                .ms_pacman
                .render(matrix, &time_str, hours, minutes, &font, effective_size),
            37 => self.words.render(
                matrix,
                hours,
                minutes,
                &font,
                effective_size,
                &context.config.settings.read().system.lang,
            ),
            27 => self
                .versus
                .render(matrix, hours, minutes, &font, effective_size),
            28 => {
                self.slot_machine
                    .render(matrix, &time_str, hours, minutes, &font, effective_size)
            }
            29 => self
                .tetris_gb
                .render(matrix, &time_str, &font, effective_size),
            _ => {
                if is_tate {
                    self.base_renderer.render_tate_time(
                        matrix,
                        hours,
                        minutes,
                        seconds,
                        self.time_theme,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        None,
                        None,
                    );
                } else {
                    self.base_renderer.render_text(
                        matrix,
                        &time_str,
                        self.time_theme,
                        effective_size,
                        self.time_offset_x,
                        self.time_offset_y,
                        None,
                        None,
                    );
                }
            }
        }
    }

    fn on_display_geometry_changed(&mut self, geometry: &crate::core::types::DisplayGeometry) {
        let w = geometry.logical_width;
        let h = geometry.logical_height;
        self.cyberpunk = CyberpunkRenderer::new(w, h);
        self.true_matrix = TrueMatrixRenderer::new(w, h);
        self.pong = PongClock::new(w, h);
        self.tetris = TetrisClock::new(false);
        self.tetris_gb = TetrisClock::new(true);
        self.flip.reset();
    }
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_clock_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "clock",
            name: "ClockEngine",
            category: "info",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "theme",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Theme",
                    description: "Clock theme",
                    default_value: "0",
                    min_val: Some("0"),
                    options_endpoint: Some("/api/themes"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "format",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Format",
                    description: "Time format",
                    default_value: "%H:%M:%S",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "font",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Font",
                    description: "Font file path",
                    default_value: "PressStart2P.ttf",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    options_endpoint: Some("/api/fonts"),
                    visible_when: Some("theme!=30,31,32,33,37"),
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "timezone",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Timezone",
                    description: "Select timezone or region",
                    default_value: "Europe/Paris",
                    options_endpoint: Some("/api/timezones"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "size",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Size",
                    description: "Font size scale",
                    default_value: "2",
                    min_val: Some("1"),
                    max_val: Some("10"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "clock_speed",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Animation Speed",
                    description:
                        "Animation speed in percent (Tetris block fall, Pac-Man sweep, Mario run); lower is slower",
                    default_value: "100",
                    min_val: Some("25"),
                    max_val: Some("300"),
                    step: Some("25"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "clock_glow",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Glow Outline",
                    description: "Halo around the digits, the effect the Matrix face uses",
                    default_value: "0",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Off",
                            value: "0",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Neon (Matrix style)",
                            value: "1",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Custom outline color",
                            value: "2",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    visible_when: Some("theme!=30,31,32,33"),
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "clock_glow_color",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Outline Color",
                    description: "Color of the outline drawn around the digits",
                    default_value: "#00FF41",
                    visible_when: Some("clock_glow=2"),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color_1",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Primary Color",
                    description: "Hex color for main clock",
                    default_value: "#FFFFFF",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color_2",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Secondary Color",
                    description: "Hex color for secondary elements",
                    default_value: "#FFFFFF",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_x",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "X Offset",
                    description: "Horizontal shift",
                    default_value: "0",
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_y",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Y Offset",
                    description: "Vertical shift",
                    default_value: "0",
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            // We pass 0, 0 since width/height are handled dynamically now or don't matter in new()
            Box::new(ClockEngine::new(64, 32))
        },
    }
}
