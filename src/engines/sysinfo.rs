use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use linkme::distributed_slice;
use std::time::{Duration, Instant};
use sysinfo::System;

pub struct SysInfoEngine {
    base_renderer: BaseRenderer,
    sys: System,
    last_update: Instant,
    cached_cpu: f32,
    cached_ram_pct: f32,
    cached_temp_c: f32,
    cached_uptime_sec: u64,

    theme: i32,
    show_cpu: bool,
    show_ram: bool,
    show_temp: bool,
    show_uptime: bool,
    temp_unit: String,
    offset_x: i32,
    offset_y: i32,
}

impl SysInfoEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        let mut sys = System::new_all();
        sys.refresh_cpu_all();
        sys.refresh_memory();

        Self {
            base_renderer: BaseRenderer::new(),
            sys,
            last_update: Instant::now() - Duration::from_secs(5),
            cached_cpu: 10.0,
            cached_ram_pct: 35.0,
            cached_temp_c: 42.0,
            cached_uptime_sec: 0,

            theme: 0,
            show_cpu: true,
            show_ram: true,
            show_temp: true,
            show_uptime: true,
            temp_unit: "C".to_string(),
            offset_x: 0,
            offset_y: 0,
        }
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.theme = config.get_int("theme", 0);
        self.show_cpu = config.get_bool("show_cpu", true);
        self.show_ram = config.get_bool("show_ram", true);
        self.show_temp = config.get_bool("show_temp", true);
        self.show_uptime = config.get_bool("show_uptime", true);
        self.temp_unit = config.get_string("temp_unit", "C");
        self.offset_x = config.get_int("offset_x", 0);
        self.offset_y = config.get_int("offset_y", 0);
    }

    fn sample_metrics(&mut self) {
        if self.last_update.elapsed() >= Duration::from_millis(800) {
            self.sys.refresh_cpu_all();
            self.sys.refresh_memory();

            self.cached_cpu = self.sys.global_cpu_usage();

            let total_mem = self.sys.total_memory();
            let used_mem = self.sys.used_memory();
            self.cached_ram_pct = if total_mem > 0 {
                (used_mem as f32 / total_mem as f32) * 100.0
            } else {
                0.0
            };

            // Read RPi thermal zone temperature
            self.cached_temp_c = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .map(|milli| milli / 1000.0)
                .unwrap_or(45.0);

            // Read Uptime
            self.cached_uptime_sec = std::fs::read_to_string("/proc/uptime")
                .ok()
                .and_then(|s| {
                    s.split_whitespace()
                        .next()
                        .and_then(|u| u.parse::<f64>().ok())
                })
                .map(|u| u as u64)
                .unwrap_or_else(|| System::uptime());

            self.last_update = Instant::now();
        }
    }

    fn get_metric_color(val: f32, warn_thresh: f32, crit_thresh: f32) -> (u8, u8, u8) {
        if val < warn_thresh {
            (0, 235, 120) // Neon Green (Healthy)
        } else if val < crit_thresh {
            (255, 195, 0) // Amber / Yellow (Warning)
        } else {
            (255, 45, 45) // Vivid Red (Critical)
        }
    }

    fn draw_gauge_bar(
        matrix: &mut dyn MatrixBackend,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        percent: f32,
        color: (u8, u8, u8),
    ) {
        let p = percent.clamp(0.0, 100.0);
        // Border track (dark grey)
        for ix in x..(x + w) {
            matrix.set_pixel(ix, y, 50, 55, 70);
            matrix.set_pixel(ix, y + h - 1, 50, 55, 70);
        }
        for iy in y..(y + h) {
            matrix.set_pixel(x, iy, 50, 55, 70);
            matrix.set_pixel(x + w - 1, iy, 50, 55, 70);
        }

        let inner_w = (w - 2).max(0);
        let fill_w = ((p / 100.0) * inner_w as f32) as i32;
        for iy in (y + 1)..(y + h - 1) {
            for ix in (x + 1)..(x + 1 + fill_w) {
                matrix.set_pixel(ix, iy, color.0, color.1, color.2);
            }
        }
    }

    fn draw_bitmap_char(
        matrix: &mut dyn MatrixBackend,
        c: char,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) -> i32 {
        let font_data: [(char, [u8; 5]); 36] = [
            ('0', [0x7C, 0x82, 0x82, 0x82, 0x7C]),
            ('1', [0x00, 0x42, 0xFE, 0x02, 0x00]),
            ('2', [0x46, 0x8A, 0x92, 0x92, 0x62]),
            ('3', [0x84, 0x82, 0x92, 0xB2, 0xCC]),
            ('4', [0x18, 0x28, 0x48, 0xFE, 0x08]),
            ('5', [0xE4, 0xA2, 0xA2, 0xA2, 0x9C]),
            ('6', [0x3C, 0x52, 0x92, 0x92, 0x0C]),
            ('7', [0x80, 0x8E, 0x90, 0xA0, 0xC0]),
            ('8', [0x6C, 0x92, 0x92, 0x92, 0x6C]),
            ('9', [0x60, 0x92, 0x92, 0x94, 0x78]),
            ('A', [0x7E, 0x88, 0x88, 0x88, 0x7E]),
            ('B', [0xFE, 0x92, 0x92, 0x92, 0x6C]),
            ('C', [0x7C, 0x82, 0x82, 0x82, 0x44]),
            ('D', [0xFE, 0x82, 0x82, 0x82, 0x7C]),
            ('E', [0xFE, 0x92, 0x92, 0x92, 0x82]),
            ('F', [0xFE, 0x90, 0x90, 0x90, 0x80]),
            ('G', [0x7C, 0x82, 0x92, 0x92, 0x5C]),
            ('H', [0xFE, 0x10, 0x10, 0x10, 0xFE]),
            ('I', [0x00, 0x82, 0xFE, 0x82, 0x00]),
            ('M', [0xFE, 0x40, 0x30, 0x40, 0xFE]),
            ('P', [0xFE, 0x90, 0x90, 0x90, 0x60]),
            ('R', [0xFE, 0x90, 0x98, 0x94, 0x62]),
            ('T', [0x80, 0x80, 0xFE, 0x80, 0x80]),
            ('U', [0xFC, 0x02, 0x02, 0x02, 0xFC]),
            (':', [0x00, 0x66, 0x66, 0x00, 0x00]),
            ('%', [0xC6, 0xC8, 0x10, 0x26, 0xC6]),
            ('.', [0x00, 0x06, 0x06, 0x00, 0x00]),
            ('-', [0x10, 0x10, 0x10, 0x10, 0x10]),
            (' ', [0x00, 0x00, 0x00, 0x00, 0x00]),
            ('h', [0xFE, 0x10, 0x20, 0x20, 0x1E]),
            ('m', [0x3E, 0x20, 0x1E, 0x20, 0x1E]),
            ('s', [0x22, 0x2A, 0x2A, 0x2A, 0x14]),
            ('K', [0xFE, 0x10, 0x28, 0x44, 0x82]),
            ('B', [0xFE, 0x92, 0x92, 0x92, 0x6C]),
            ('G', [0x7C, 0x82, 0x92, 0x92, 0x5C]),
            ('O', [0x7C, 0x82, 0x82, 0x82, 0x7C]),
        ];

        let cols = font_data
            .iter()
            .find(|(ch, _)| *ch == c)
            .map(|(_, arr)| *arr)
            .unwrap_or([0x00, 0x00, 0x00, 0x00, 0x00]);

        for (col_idx, col_byte) in cols.iter().enumerate() {
            for row_idx in 0..8 {
                if (col_byte & (1 << (7 - row_idx))) != 0 {
                    matrix.set_pixel(
                        x + col_idx as i32,
                        y + row_idx as i32,
                        color.0,
                        color.1,
                        color.2,
                    );
                }
            }
        }
        6 // advance width
    }

    fn draw_bitmap_string(
        matrix: &mut dyn MatrixBackend,
        text: &str,
        mut x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) {
        for c in text.chars() {
            x += Self::draw_bitmap_char(matrix, c, x, y, color);
        }
    }
}

impl Engine for SysInfoEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn is_realtime(&self) -> bool {
        true
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }

    fn update(&mut self, _context: &mut EngineContext) {
        self.sample_metrics();
    }

    fn render(&mut self, context: &mut EngineContext) {
        let matrix = &mut *context.matrix;
        matrix.clear();

        let cpu = self.cached_cpu;
        let ram = self.cached_ram_pct;
        let temp = self.cached_temp_c;
        let uptime = self.cached_uptime_sec;

        let cpu_col = Self::get_metric_color(cpu, 60.0, 80.0);
        let ram_col = Self::get_metric_color(ram, 70.0, 85.0);
        let temp_col = Self::get_metric_color(temp, 55.0, 70.0);
        let label_col = (140, 155, 185);

        let bx = 2 + self.offset_x;
        let by = 1 + self.offset_y;

        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let d_temp = if self.temp_unit.eq_ignore_ascii_case("F") {
            temp * 1.8 + 32.0
        } else {
            temp
        };
        let t_char = if self.temp_unit.eq_ignore_ascii_case("F") {
            'F'
        } else {
            'C'
        };
        let t_str = format!("{:.0}{}", d_temp, t_char);

        let hrs = uptime / 3600;
        let mins = (uptime % 3600) / 60;
        let up_str = if hrs > 0 {
            format!("{}h{:02}", hrs, mins)
        } else {
            format!("{}m{:02}", mins, uptime % 60)
        };

        match self.theme {
            1 => {
                // Cyberpunk Neon theme
                let cyan = (0, 240, 255);
                let purple = (200, 50, 255);

                // Futuristic corner brackets across full width & height
                for ix in 0..8 {
                    matrix.set_pixel(ix, 0, cyan.0, cyan.1, cyan.2);
                    matrix.set_pixel(w - 1 - ix, 0, cyan.0, cyan.1, cyan.2);
                    matrix.set_pixel(ix, h - 1, purple.0, purple.1, purple.2);
                    matrix.set_pixel(w - 1 - ix, h - 1, purple.0, purple.1, purple.2);
                }
                for iy in 0..6 {
                    matrix.set_pixel(0, iy, cyan.0, cyan.1, cyan.2);
                    matrix.set_pixel(w - 1, iy, cyan.0, cyan.1, cyan.2);
                    matrix.set_pixel(0, h - 1 - iy, purple.0, purple.1, purple.2);
                    matrix.set_pixel(w - 1, h - 1 - iy, purple.0, purple.1, purple.2);
                }

                if w >= 100 {
                    // Widescreen (128x32+): 2 Columns + Level Meter on right
                    let x1 = 6 + self.offset_x;
                    let x2 = (w / 2) + self.offset_x;
                    let y1 = 4 + self.offset_y;
                    let y2 = 18 + self.offset_y;

                    Self::draw_bitmap_string(matrix, "CPU:", x1, y1, cyan);
                    Self::draw_bitmap_string(matrix, &format!("{:.0}%", cpu), x1 + 24, y1, cpu_col);

                    Self::draw_bitmap_string(matrix, "RAM:", x1, y2, purple);
                    Self::draw_bitmap_string(matrix, &format!("{:.0}%", ram), x1 + 24, y2, ram_col);

                    Self::draw_bitmap_string(matrix, "TMP:", x2, y1, (255, 140, 0));
                    Self::draw_bitmap_string(matrix, &t_str, x2 + 24, y1, temp_col);

                    Self::draw_bitmap_string(matrix, "UPT:", x2, y2, cyan);
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:02}:{:02}", hrs, mins),
                        x2 + 24,
                        y2,
                        (0, 220, 200),
                    );

                    // Level meter on right
                    let bar_h = (h - 8).max(4);
                    let fill_h = ((cpu / 100.0) * bar_h as f32) as i32;
                    for iy in 0..fill_h {
                        matrix.set_pixel(w - 4, h - 4 - iy, cpu_col.0, cpu_col.1, cpu_col.2);
                        matrix.set_pixel(w - 3, h - 4 - iy, cpu_col.0, cpu_col.1, cpu_col.2);
                    }
                } else {
                    // Compact (64x32)
                    let cx = 4 + self.offset_x;
                    let cy = 4 + self.offset_y;

                    Self::draw_bitmap_string(matrix, "C:", cx, cy, cyan);
                    Self::draw_bitmap_string(matrix, &format!("{:.0}%", cpu), cx + 12, cy, cpu_col);

                    Self::draw_bitmap_string(matrix, "R:", (w / 2) + self.offset_x, cy, purple);
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:.0}%", ram),
                        (w / 2) + 12 + self.offset_x,
                        cy,
                        ram_col,
                    );

                    let y2 = cy + 14;
                    Self::draw_bitmap_string(matrix, &t_str, cx, y2, temp_col);
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:02}:{:02}", hrs, mins),
                        (w / 2) + self.offset_x,
                        y2,
                        (0, 220, 200),
                    );

                    let bar_h = (h - 8).max(4);
                    let fill_h = ((cpu / 100.0) * bar_h as f32) as i32;
                    for iy in 0..fill_h {
                        matrix.set_pixel(w - 3, h - 4 - iy, cpu_col.0, cpu_col.1, cpu_col.2);
                    }
                }
            }
            2 => {
                // Compact Grid Theme
                if w >= 100 && h <= 36 {
                    // Widescreen: 4 Columns Side-by-Side
                    let col_w = (w - 4) / 4;
                    for i in 1..4 {
                        let sep_x = 2 + (i * col_w);
                        for iy in 2..(h - 2) {
                            matrix.set_pixel(sep_x, iy, 40, 45, 60);
                        }
                    }

                    // Col 1: CPU
                    let x0 = 3 + self.offset_x;
                    Self::draw_bitmap_string(matrix, "CPU", x0, 4 + self.offset_y, label_col);
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:.0}%", cpu),
                        x0,
                        16 + self.offset_y,
                        cpu_col,
                    );

                    // Col 2: RAM
                    let x1 = 3 + col_w + self.offset_x;
                    Self::draw_bitmap_string(matrix, "RAM", x1, 4 + self.offset_y, label_col);
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:.0}%", ram),
                        x1,
                        16 + self.offset_y,
                        ram_col,
                    );

                    // Col 3: TMP
                    let x2 = 3 + (2 * col_w) + self.offset_x;
                    Self::draw_bitmap_string(matrix, "TMP", x2, 4 + self.offset_y, label_col);
                    Self::draw_bitmap_string(matrix, &t_str, x2, 16 + self.offset_y, temp_col);

                    // Col 4: UPT
                    let x3 = 3 + (3 * col_w) + self.offset_x;
                    Self::draw_bitmap_string(matrix, "UPT", x3, 4 + self.offset_y, label_col);
                    Self::draw_bitmap_string(
                        matrix,
                        &up_str,
                        x3,
                        16 + self.offset_y,
                        (0, 190, 255),
                    );
                } else {
                    // 2x2 Grid with dynamic split
                    let mid_x = w / 2;
                    let mid_y = h / 2;

                    for iy in 2..(h - 2) {
                        matrix.set_pixel(mid_x, iy, 40, 45, 60);
                    }
                    for ix in 2..(w - 2) {
                        matrix.set_pixel(ix, mid_y, 40, 45, 60);
                    }

                    // Quad 1: CPU
                    Self::draw_bitmap_string(
                        matrix,
                        "CPU",
                        2 + self.offset_x,
                        2 + self.offset_y,
                        label_col,
                    );
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:.0}%", cpu),
                        2 + self.offset_x,
                        9 + self.offset_y,
                        cpu_col,
                    );

                    // Quad 2: RAM
                    Self::draw_bitmap_string(
                        matrix,
                        "RAM",
                        mid_x + 3 + self.offset_x,
                        2 + self.offset_y,
                        label_col,
                    );
                    Self::draw_bitmap_string(
                        matrix,
                        &format!("{:.0}%", ram),
                        mid_x + 3 + self.offset_x,
                        9 + self.offset_y,
                        ram_col,
                    );

                    // Quad 3: TEMP
                    Self::draw_bitmap_string(
                        matrix,
                        "TMP",
                        2 + self.offset_x,
                        mid_y + 3 + self.offset_y,
                        label_col,
                    );
                    Self::draw_bitmap_string(
                        matrix,
                        &t_str,
                        2 + self.offset_x,
                        mid_y + 10 + self.offset_y,
                        temp_col,
                    );

                    // Quad 4: UPTIME
                    Self::draw_bitmap_string(
                        matrix,
                        "UPT",
                        mid_x + 3 + self.offset_x,
                        mid_y + 3 + self.offset_y,
                        label_col,
                    );
                    Self::draw_bitmap_string(
                        matrix,
                        &up_str,
                        mid_x + 3 + self.offset_x,
                        mid_y + 10 + self.offset_y,
                        (0, 190, 255),
                    );
                }
            }
            _ => {
                // Theme 0: HUD Gauges (Default)
                if w >= 100 {
                    // Widescreen (128x32+): 2 Balanced Columns
                    let col_w = (w / 2) - 4;
                    let x1 = 2 + self.offset_x;
                    let x2 = (w / 2) + 2 + self.offset_x;
                    let y1 = 4 + self.offset_y;
                    let y2 = 18 + self.offset_y;

                    let bar_w = (col_w - 46).max(12);

                    // Column 1 - Row 1: CPU
                    if self.show_cpu {
                        Self::draw_bitmap_string(matrix, "CPU", x1, y1, label_col);
                        Self::draw_gauge_bar(matrix, x1 + 20, y1, bar_w, 7, cpu, cpu_col);
                        Self::draw_bitmap_string(
                            matrix,
                            &format!("{:2.0}%", cpu),
                            x1 + 22 + bar_w,
                            y1,
                            cpu_col,
                        );
                    }

                    // Column 1 - Row 2: RAM
                    if self.show_ram {
                        Self::draw_bitmap_string(matrix, "RAM", x1, y2, label_col);
                        Self::draw_gauge_bar(matrix, x1 + 20, y2, bar_w, 7, ram, ram_col);
                        Self::draw_bitmap_string(
                            matrix,
                            &format!("{:2.0}%", ram),
                            x1 + 22 + bar_w,
                            y2,
                            ram_col,
                        );
                    }

                    // Column 2 - Row 1: TEMP
                    if self.show_temp {
                        Self::draw_bitmap_string(matrix, "TMP", x2, y1, label_col);
                        let temp_pct = ((temp - 20.0) * (100.0 / 60.0)).clamp(0.0, 100.0);
                        Self::draw_gauge_bar(matrix, x2 + 20, y1, bar_w, 7, temp_pct, temp_col);
                        Self::draw_bitmap_string(matrix, &t_str, x2 + 22 + bar_w, y1, temp_col);
                    }

                    // Column 2 - Row 2: UPTIME
                    if self.show_uptime {
                        Self::draw_bitmap_string(matrix, "UPT", x2, y2, label_col);
                        Self::draw_bitmap_string(matrix, &up_str, x2 + 22, y2, (0, 190, 255));
                    }
                } else if w < 48 || h > (w * 3) / 2 || (w <= 64 && h >= 64) {
                    // Portrait / Tate Stacked Layout (e.g. 32x64, 32x128, 64x64, 64x128)
                    let step_y = h / 4;
                    let base_y = 2 + self.offset_y;
                    let bar_h = if step_y > 16 { 5 } else { 3 };
                    let bar_w = (w - 4).max(4);

                    // Row 1: CPU
                    if self.show_cpu {
                        Self::draw_bitmap_string(
                            matrix,
                            "CPU",
                            2 + self.offset_x,
                            base_y,
                            label_col,
                        );
                        let buf = format!("{:2.0}%", cpu);
                        let val_x = w - (buf.len() as i32 * 6 + 2) + self.offset_x;
                        Self::draw_bitmap_string(matrix, &buf, val_x, base_y, cpu_col);
                        Self::draw_gauge_bar(
                            matrix,
                            2 + self.offset_x,
                            base_y + 8,
                            bar_w,
                            bar_h,
                            cpu,
                            cpu_col,
                        );
                    }

                    // Row 2: RAM
                    if self.show_ram {
                        let y2 = base_y + step_y;
                        Self::draw_bitmap_string(matrix, "RAM", 2 + self.offset_x, y2, label_col);
                        let buf = format!("{:2.0}%", ram);
                        let val_x = w - (buf.len() as i32 * 6 + 2) + self.offset_x;
                        Self::draw_bitmap_string(matrix, &buf, val_x, y2, ram_col);
                        Self::draw_gauge_bar(
                            matrix,
                            2 + self.offset_x,
                            y2 + 8,
                            bar_w,
                            bar_h,
                            ram,
                            ram_col,
                        );
                    }

                    // Row 3: TEMP
                    if self.show_temp {
                        let y3 = base_y + step_y * 2;
                        Self::draw_bitmap_string(matrix, "TMP", 2 + self.offset_x, y3, label_col);
                        let val_x = w - (t_str.len() as i32 * 6 + 2) + self.offset_x;
                        Self::draw_bitmap_string(matrix, &t_str, val_x, y3, temp_col);
                        let temp_pct = ((temp - 20.0) * (100.0 / 60.0)).clamp(0.0, 100.0);
                        Self::draw_gauge_bar(
                            matrix,
                            2 + self.offset_x,
                            y3 + 8,
                            bar_w,
                            bar_h,
                            temp_pct,
                            temp_col,
                        );
                    }

                    // Row 4: UPTIME
                    if self.show_uptime {
                        let y4 = base_y + step_y * 3;
                        Self::draw_bitmap_string(matrix, "UPT", 2 + self.offset_x, y4, label_col);
                        let val_x = w - (up_str.len() as i32 * 6 + 2) + self.offset_x;
                        Self::draw_bitmap_string(matrix, &up_str, val_x, y4, (0, 190, 255));
                        Self::draw_gauge_bar(
                            matrix,
                            2 + self.offset_x,
                            y4 + 8,
                            bar_w,
                            bar_h,
                            100.0,
                            (0, 190, 255),
                        );
                    }
                } else {
                    // Compact (64x32)
                    let base_x = 2 + self.offset_x;
                    let base_y = 2 + self.offset_y;
                    let bar_w = (w - 46).max(10);

                    if self.show_cpu {
                        Self::draw_bitmap_string(matrix, "CPU", base_x, base_y, label_col);
                        Self::draw_gauge_bar(matrix, base_x + 20, base_y, bar_w, 6, cpu, cpu_col);
                        Self::draw_bitmap_string(
                            matrix,
                            &format!("{:2.0}%", cpu),
                            base_x + 22 + bar_w,
                            base_y,
                            cpu_col,
                        );
                    }

                    if self.show_ram {
                        let y2 = base_y + 10;
                        Self::draw_bitmap_string(matrix, "RAM", base_x, y2, label_col);
                        Self::draw_gauge_bar(matrix, base_x + 20, y2, bar_w, 6, ram, ram_col);
                        Self::draw_bitmap_string(
                            matrix,
                            &format!("{:2.0}%", ram),
                            base_x + 22 + bar_w,
                            y2,
                            ram_col,
                        );
                    }

                    let y3 = base_y + 20;
                    if self.show_temp {
                        Self::draw_bitmap_string(matrix, &t_str, base_x, y3, temp_col);
                    }
                    if self.show_uptime {
                        Self::draw_bitmap_string(
                            matrix,
                            &up_str,
                            (w / 2) + 2 + self.offset_x,
                            y3,
                            (0, 190, 255),
                        );
                    }
                }
            }
        }
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_sysinfo_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "system_info",
            name: "System Monitor",
            category: "system",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            allows_overlay: true,
            allow_rotation: true,
            ..Default::default()
        },
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Layout Style",
                    description: "Visual style layout (HUD Bars, Cyberpunk Neon, Compact Grid)",
                    default_value: "0",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "HUD Bars & Gauges",
                            value: "0",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Cyberpunk Neon",
                            value: "1",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Compact Grid (2x2)",
                            value: "2",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_cpu",
                    field_type: ConfigType::Boolean,
                    label: "Show CPU",
                    description: "Display processor load percentage & gauge",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_ram",
                    field_type: ConfigType::Boolean,
                    label: "Show RAM",
                    description: "Display memory usage percentage & gauge",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_temp",
                    field_type: ConfigType::Boolean,
                    label: "Show Temperature",
                    description: "Display core/environment temperature",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_uptime",
                    field_type: ConfigType::Boolean,
                    label: "Show Uptime",
                    description: "Display running uptime counter",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "temp_unit",
                    field_type: ConfigType::Options,
                    label: "Temperature Unit",
                    description: "Celsius (°C) or Fahrenheit (°F)",
                    default_value: "C",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Celsius (°C)",
                            value: "C",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Fahrenheit (°F)",
                            value: "F",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "offset_x",
                    field_type: ConfigType::Integer,
                    label: "X Offset",
                    description: "Horizontal pixel shift",
                    default_value: "0",
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "offset_y",
                    field_type: ConfigType::Integer,
                    label: "Y Offset",
                    description: "Vertical pixel shift",
                    default_value: "0",
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn Engine> { Box::new(SysInfoEngine::new(64, 32)) },
    }
}
