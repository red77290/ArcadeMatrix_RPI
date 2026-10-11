//! Time in words, adapted from the 64x64 "Clockwise" clockface cw-cf-0x02.
//!
//! The hour on one line and the minutes under it, a rule beneath, then the date. Wording and date
//! order come from the localisation layer, so the face follows the language the sign is set to.
//! Laid out for a wide panel; smaller ones get the notice. Matches the ESP32 face.

use crate::core::matrix::IDrawingSurface;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use chrono::{Datelike, Local};

const ACCENT: (u8, u8, u8) = (72, 176, 200);

pub struct WordsClock;

impl WordsClock {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
        lang: &str,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        if w < 192 || h < 64 {
            super::wide_only_notice(matrix, font, scale);
            return;
        }
        matrix.clear();

        let (hour_words, minute_words) = spoken_time(hours, minutes, lang);
        let s = scale.max(1) as f32;

        centre(matrix, &hour_words, font, s, h * 22 / 100, ACCENT, w);
        centre(
            matrix,
            &minute_words,
            font,
            s,
            h * 45 / 100,
            (255, 255, 255),
            w,
        );

        let rule_w = w * 3 / 4;
        let rule_y = h * 63 / 100;
        for x in ((w - rule_w) / 2)..((w - rule_w) / 2 + rule_w) {
            matrix.set_pixel(x, rule_y, 255, 255, 255);
        }

        let now = Local::now();
        let date = date_line(
            now.weekday().num_days_from_sunday() as usize,
            now.month0() as usize,
            now.day() as i32,
            lang,
        );
        centre(matrix, &date, font, s, h * 80 / 100, ACCENT, w);
    }
}

fn centre(
    matrix: &mut dyn IDrawingSurface,
    text: &str,
    font: &ArcadeFont<'_>,
    scale: f32,
    centre_y: i32,
    color: (u8, u8, u8),
    panel_w: i32,
) {
    if text.is_empty() {
        return;
    }
    let (_, tw, th) = font.get_pixel_map(text, scale);
    BaseRenderer::draw_text_at(
        matrix,
        text,
        font,
        scale,
        (panel_w - tw as i32) / 2,
        centre_y - th as i32 / 2,
        color,
        (0, 0, 0),
    );
}

const EN_HOURS: [&str; 13] = [
    "twelve", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve",
];
const EN_UNITS: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const EN_TENS: [&str; 6] = ["", "ten", "twenty", "thirty", "forty", "fifty"];
const FR_HOURS: [&str; 13] = [
    "minuit", "une", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix",
    "onze", "midi",
];
const FR_UNITS: [&str; 20] = [
    "zero", "une", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze",
    "douze", "treize", "quatorze", "quinze", "seize", "dix-sept", "dix-huit", "dix-neuf",
];
const FR_TENS: [&str; 6] = ["", "dix", "vingt", "trente", "quarante", "cinquante"];
const ES_HOURS: [&str; 13] = [
    "doce", "una", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez",
    "once", "doce",
];
const ES_UNITS: [&str; 20] = [
    "cero",
    "uno",
    "dos",
    "tres",
    "cuatro",
    "cinco",
    "seis",
    "siete",
    "ocho",
    "nueve",
    "diez",
    "once",
    "doce",
    "trece",
    "catorce",
    "quince",
    "dieciseis",
    "diecisiete",
    "dieciocho",
    "diecinueve",
];
const ES_TENS: [&str; 6] = ["", "diez", "veinte", "treinta", "cuarenta", "cincuenta"];

pub(crate) fn spell_minutes(m: u32, lang: &str) -> String {
    let m = m as usize;
    if m < 20 {
        return match lang {
            "fr" => FR_UNITS[m],
            "es" => ES_UNITS[m],
            _ => EN_UNITS[m],
        }
        .to_string();
    }
    let (tens, units) = (m / 10, m % 10);
    let tens_word = match lang {
        "fr" => FR_TENS[tens],
        "es" => ES_TENS[tens],
        _ => EN_TENS[tens],
    };
    if units == 0 {
        return tens_word.to_string();
    }
    let unit_word = match lang {
        "fr" => FR_UNITS[units],
        "es" => ES_UNITS[units],
        _ => EN_UNITS[units],
    };
    match lang {
        "fr" => {
            if units == 1 {
                format!("{} et {}", tens_word, unit_word)
            } else {
                format!("{}-{}", tens_word, unit_word)
            }
        }
        "es" => {
            if tens == 2 {
                format!("veinti{}", unit_word)
            } else {
                format!("{} y {}", tens_word, unit_word)
            }
        }
        _ => format!("{} {}", tens_word, unit_word),
    }
}

/// The same wording as the ESP32 firmware, so both signs read alike.
pub fn spoken_time(hours: u32, minutes: u32, lang: &str) -> (String, String) {
    let h24 = hours % 24;
    let m = minutes % 60;
    let h12 = (h24 % 12) as usize;

    match lang {
        "fr" => {
            if m == 0 && (h24 == 0 || h24 == 12) {
                return (
                    if h24 == 0 { "minuit" } else { "midi" }.into(),
                    String::new(),
                );
            }
            let hour = FR_HOURS[if h12 == 0 {
                if h24 == 0 {
                    0
                } else {
                    12
                }
            } else {
                h12
            }];
            let mins = match m {
                0 => "heures".to_string(),
                15 => "heures et quart".to_string(),
                30 => "heures et demie".to_string(),
                _ => format!("heures {}", spell_minutes(m, lang)),
            };
            (hour.to_string(), mins)
        }
        "es" => {
            if m == 0 && (h24 == 0 || h24 == 12) {
                return (
                    if h24 == 0 { "medianoche" } else { "mediodia" }.into(),
                    String::new(),
                );
            }
            let hour = ES_HOURS[if h12 == 0 { 0 } else { h12 }];
            let mins = match m {
                0 => "en punto".to_string(),
                15 => "y cuarto".to_string(),
                30 => "y media".to_string(),
                _ => format!("y {}", spell_minutes(m, lang)),
            };
            (hour.to_string(), mins)
        }
        _ => {
            if m == 0 && h24 == 0 {
                return ("midnight".into(), String::new());
            }
            if m == 0 && h24 == 12 {
                return ("noon".into(), String::new());
            }
            let hour = EN_HOURS[if h12 == 0 { 0 } else { h12 }];
            let mins = if m == 0 {
                "o'clock".to_string()
            } else if m == 30 {
                if h24 == 0 || h24 == 12 {
                    "thirty".into()
                } else {
                    "a half".into()
                }
            } else if m < 10 {
                format!("oh {}", EN_UNITS[m as usize])
            } else {
                spell_minutes(m, lang)
            };
            (hour.to_string(), mins)
        }
    }
}

/// Month before day in English, day before month in French and Spanish.
pub fn date_line(weekday: usize, month0: usize, day: i32, lang: &str) -> String {
    const EN_DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    const FR_DAYS: [&str; 7] = ["DIM", "LUN", "MAR", "MER", "JEU", "VEN", "SAM"];
    const ES_DAYS: [&str; 7] = ["DOM", "LUN", "MAR", "MIE", "JUE", "VIE", "SAB"];
    const EN_MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    const FR_MONTHS: [&str; 12] = [
        "JANV", "FEVR", "MARS", "AVR", "MAI", "JUIN", "JUIL", "AOUT", "SEPT", "OCT", "NOV", "DEC",
    ];
    const ES_MONTHS: [&str; 12] = [
        "ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC",
    ];
    let (days, months) = match lang {
        "fr" => (&FR_DAYS, &FR_MONTHS),
        "es" => (&ES_DAYS, &ES_MONTHS),
        _ => (&EN_DAYS, &EN_MONTHS),
    };
    if lang == "en" {
        format!("{} {} {}", days[weekday % 7], months[month0 % 12], day)
    } else {
        format!("{} {} {}", days[weekday % 7], day, months[month0 % 12])
    }
}

impl Default for WordsClock {
    fn default() -> Self {
        Self::new()
    }
}
