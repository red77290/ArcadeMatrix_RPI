//! Centralized internationalization (i18n) module for ArcadeMatrix RPi.
//! Single source of truth for all translations (Weather, WordClock, Noise, etc.).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
    Es,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_lowercase().as_str() {
            "en" => Lang::En,
            "es" => Lang::Es,
            _ => Lang::Fr,
        }
    }

    pub fn as_code(&self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }
}

// ----------------------------------------------------------------------------
// 1. Weather Day Labels
// ----------------------------------------------------------------------------
pub fn weather_day_label(
    lang: Lang,
    day_of_week: usize, // 0 = Sun, 1 = Mon, ..., 6 = Sat
    is_today: bool,
    is_tomorrow: bool,
) -> &'static str {
    if is_today {
        return match lang {
            Lang::Fr => "AUJ.",
            Lang::En => "TODAY",
            Lang::Es => "HOY",
        };
    }
    if is_tomorrow {
        return match lang {
            Lang::Fr => "DEM.",
            Lang::En => "TOM.",
            Lang::Es => "MAÑ.",
        };
    }
    match lang {
        Lang::Fr => match day_of_week % 7 {
            0 => "DIM",
            1 => "LUN",
            2 => "MAR",
            3 => "MER",
            4 => "JEU",
            5 => "VEN",
            _ => "SAM",
        },
        Lang::En => match day_of_week % 7 {
            0 => "SUN",
            1 => "MON",
            2 => "TUE",
            3 => "WED",
            4 => "THU",
            5 => "FRI",
            _ => "SAT",
        },
        Lang::Es => match day_of_week % 7 {
            0 => "DOM",
            1 => "LUN",
            2 => "MAR",
            3 => "MIÉ",
            4 => "JUE",
            5 => "VIE",
            _ => "SÁB",
        },
    }
}

// ----------------------------------------------------------------------------
// 2. Weather Conditions
// ----------------------------------------------------------------------------
pub fn weather_condition(lang: Lang, raw_condition: &str) -> &'static str {
    let lower = raw_condition.trim().to_lowercase();
    match lang {
        Lang::Fr => {
            if lower.contains("clear") || lower.contains("sun") {
                "Soleil"
            } else if lower.contains("few clouds") || lower.contains("scattered") {
                "Eclairc."
            } else if lower.contains("overcast") {
                "Couvert"
            } else if lower.contains("cloud") {
                "Nuages"
            } else if lower.contains("thunder") || lower.contains("storm") {
                "Orage"
            } else if lower.contains("drizzle") {
                "Bruine"
            } else if lower.contains("rain") {
                "Pluie"
            } else if lower.contains("snow") {
                "Neige"
            } else if lower.contains("mist") {
                "Brume"
            } else if lower.contains("fog") {
                "Brouill."
            } else {
                "Variable"
            }
        }
        Lang::Es => {
            if lower.contains("clear") || lower.contains("sun") {
                "Soleado"
            } else if lower.contains("few clouds") || lower.contains("scattered") {
                "Parcial"
            } else if lower.contains("overcast") {
                "Cubierto"
            } else if lower.contains("cloud") {
                "Nubes"
            } else if lower.contains("thunder") || lower.contains("storm") {
                "Torm."
            } else if lower.contains("drizzle") {
                "Lloviz."
            } else if lower.contains("rain") {
                "Lluvia"
            } else if lower.contains("snow") {
                "Nieve"
            } else if lower.contains("mist") {
                "Bruma"
            } else if lower.contains("fog") {
                "Niebla"
            } else {
                "Variable"
            }
        }
        Lang::En => {
            if lower.contains("clear") || lower.contains("sun") {
                "Clear"
            } else if lower.contains("few clouds") || lower.contains("scattered") {
                "P.Cloudy"
            } else if lower.contains("overcast") {
                "Overcast"
            } else if lower.contains("cloud") {
                "Clouds"
            } else if lower.contains("thunder") || lower.contains("storm") {
                "Storm"
            } else if lower.contains("drizzle") {
                "Drizzle"
            } else if lower.contains("rain") {
                "Rain"
            } else if lower.contains("snow") {
                "Snow"
            } else if lower.contains("mist") {
                "Mist"
            } else if lower.contains("fog") {
                "Fog"
            } else {
                "Clear"
            }
        }
    }
}

// ----------------------------------------------------------------------------
// 3. WordClock
// ----------------------------------------------------------------------------
pub fn word_clock_lines(lang: Lang, hours: u32, minutes: u32) -> Vec<String> {
    let rounded_m = (minutes / 5) * 5;
    let past_half = minutes > 30;
    let display_h = if past_half && rounded_m != 0 {
        (hours + 1) % 24
    } else {
        hours
    };
    let read_h = display_h % 12;

    match lang {
        Lang::Fr => {
            let str_h: &str = match display_h {
                0 => "MINUIT",
                12 => "MIDI",
                _ => match read_h {
                    1 => "UNE",
                    2 => "DEUX",
                    3 => "TROIS",
                    4 => "QUATRE",
                    5 => "CINQ",
                    6 => "SIX",
                    7 => "SEPT",
                    8 => "HUIT",
                    9 => "NEUF",
                    10 => "DIX",
                    11 => "ONZE",
                    _ => "?",
                },
            };

            let str_h_suffix: &str = if display_h == 0 || display_h == 12 {
                ""
            } else if read_h == 1 {
                " HEURE"
            } else {
                " HEURES"
            };

            let str_m: String = match rounded_m {
                0 | 60 => "PILE".to_string(),
                5 if !past_half => "CINQ".to_string(),
                10 if !past_half => "DIX".to_string(),
                15 if !past_half => "ET QUART".to_string(),
                20 if !past_half => "VINGT".to_string(),
                25 if !past_half => "VINGT-CINQ".to_string(),
                30 => "ET DEMIE".to_string(),
                _ if past_half => {
                    let diff = 60 - rounded_m;
                    match diff {
                        5 => "MOINS CINQ".to_string(),
                        10 => "MOINS DIX".to_string(),
                        15 => "MOINS LE QUART".to_string(),
                        20 => "MOINS VINGT".to_string(),
                        25 => "MOINS VINGT-CINQ".to_string(),
                        _ => "MOINS CINQ".to_string(),
                    }
                }
                _ => "PILE".to_string(),
            };

            vec![
                "IL EST".to_string(),
                format!("{}{}", str_h, str_h_suffix),
                str_m,
            ]
        }
        Lang::En => {
            let str_h = match display_h {
                0 => "MIDNIGHT",
                12 => "NOON",
                _ => match read_h {
                    1 => "ONE",
                    2 => "TWO",
                    3 => "THREE",
                    4 => "FOUR",
                    5 => "FIVE",
                    6 => "SIX",
                    7 => "SEVEN",
                    8 => "EIGHT",
                    9 => "NINE",
                    10 => "TEN",
                    11 => "ELEVEN",
                    _ => "?",
                },
            };

            let str_m = match rounded_m {
                0 | 60 => "O'CLOCK".to_string(),
                5 if !past_half => "FIVE".to_string(),
                10 if !past_half => "TEN".to_string(),
                15 if !past_half => "A QUARTER".to_string(),
                20 if !past_half => "TWENTY".to_string(),
                25 if !past_half => "TWENTY-FIVE".to_string(),
                30 => "HALF".to_string(),
                _ if past_half => {
                    let diff = 60 - rounded_m;
                    match diff {
                        5 => "FIVE".to_string(),
                        10 => "TEN".to_string(),
                        15 => "A QUARTER".to_string(),
                        20 => "TWENTY".to_string(),
                        25 => "TWENTY-FIVE".to_string(),
                        _ => "FIVE".to_string(),
                    }
                }
                _ => "O'CLOCK".to_string(),
            };

            let str_conn = if rounded_m == 0 || rounded_m == 60 {
                ""
            } else if past_half {
                "TO"
            } else {
                "PAST"
            };

            let mut lines = vec!["IT IS".to_string()];
            if str_conn.is_empty() {
                if display_h == 0 || display_h == 12 {
                    lines.push(str_h.to_string());
                } else {
                    lines.push(str_h.to_string());
                    lines.push(str_m);
                }
            } else {
                lines.push(str_m);
                lines.push(str_conn.to_string());
                lines.push(str_h.to_string());
            }
            lines
        }
        Lang::Es => {
            let str_h = match display_h {
                0 => "MEDIANOCHE",
                12 => "MEDIODIA",
                _ => match read_h {
                    1 => "LA UNA",
                    2 => "LAS DOS",
                    3 => "LAS TRES",
                    4 => "LAS CUATRO",
                    5 => "LAS CINCO",
                    6 => "LAS SEIS",
                    7 => "LAS SIETE",
                    8 => "LAS OCHO",
                    9 => "LAS NUEVE",
                    10 => "LAS DIEZ",
                    11 => "LAS ONCE",
                    _ => "?",
                },
            };

            let str_m = match rounded_m {
                0 | 60 => "EN PUNTO".to_string(),
                5 if !past_half => "Y CINCO".to_string(),
                10 if !past_half => "Y DIEZ".to_string(),
                15 if !past_half => "Y CUARTO".to_string(),
                20 if !past_half => "Y VEINTE".to_string(),
                25 if !past_half => "Y VEINTICINCO".to_string(),
                30 => "Y MEDIA".to_string(),
                _ if past_half => {
                    let diff = 60 - rounded_m;
                    match diff {
                        5 => "MENOS CINCO".to_string(),
                        10 => "MENOS DIEZ".to_string(),
                        15 => "MENOS CUARTO".to_string(),
                        20 => "MENOS VEINTE".to_string(),
                        25 => "MENOS VEINTICINCO".to_string(),
                        _ => "MENOS CINCO".to_string(),
                    }
                }
                _ => "EN PUNTO".to_string(),
            };

            if display_h == 0 || display_h == 12 {
                if rounded_m == 0 || rounded_m == 60 {
                    vec!["ES LA".to_string(), str_h.to_string()]
                } else {
                    vec!["ES LA".to_string(), str_h.to_string(), str_m]
                }
            } else {
                let prefix = if read_h == 1 && display_h != 0 && display_h != 12 {
                    "ES LA"
                } else {
                    "SON LAS"
                };
                vec![prefix.to_string(), str_h.to_string(), str_m]
            }
        }
    }
}

// ----------------------------------------------------------------------------
// 4. Noise Levels (Decibel)
// ----------------------------------------------------------------------------
pub fn noise_level(lang: Lang, level_index: usize) -> &'static str {
    match lang {
        Lang::Fr => match level_index {
            0 => "SILENCE",
            1 => "PAISIBLE",
            2 => "MODERE",
            3 => "ELEVE",
            4 => "BRUYANT",
            _ => "ALERTE",
        },
        Lang::Es => match level_index {
            0 => "SILENCIO",
            1 => "TRANQUILO",
            2 => "MODERADO",
            3 => "ELEVADO",
            4 => "RUIDOSO",
            _ => "ALERTA",
        },
        Lang::En => match level_index {
            0 => "SILENCE",
            1 => "PEACEFUL",
            2 => "MODERATE",
            3 => "HIGH",
            4 => "LOUD",
            _ => "ALERT",
        },
    }
}

// ----------------------------------------------------------------------------
// 6. GNews Status Labels
// ----------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GNewsStatus {
    Ok,
    EmptyKey,
    InvalidKey,
    RateLimited,
    NetworkError,
    Loading,
}

pub fn gnews_status_label(lang: Lang, status: GNewsStatus) -> &'static str {
    match status {
        GNewsStatus::EmptyKey => match lang {
            Lang::Fr => "CLE API REQUISE",
            Lang::En => "API KEY REQUIRED",
            Lang::Es => "CLAVE API REQUERIDA",
        },
        GNewsStatus::InvalidKey => match lang {
            Lang::Fr => "CLE API INVALIDE",
            Lang::En => "INVALID API KEY",
            Lang::Es => "CLAVE API INVALIDA",
        },
        GNewsStatus::RateLimited => match lang {
            Lang::Fr => "LIMITE ATTEINTE",
            Lang::En => "RATE LIMITED",
            Lang::Es => "LIMITE SUPERADO",
        },
        GNewsStatus::NetworkError => match lang {
            Lang::Fr => "ERREUR RESEAU",
            Lang::En => "NETWORK ERROR",
            Lang::Es => "ERROR DE RED",
        },
        GNewsStatus::Loading => match lang {
            Lang::Fr => "CHARGEMENT...",
            Lang::En => "LOADING...",
            Lang::Es => "CARGANDO...",
        },
        GNewsStatus::Ok => "GNEWS LIVE",
    }
}

// ============================================================================
// Unit Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_parsing() {
        assert_eq!(Lang::from_code("fr"), Lang::Fr);
        assert_eq!(Lang::from_code("FR"), Lang::Fr);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("es"), Lang::Es);
        assert_eq!(Lang::from_code("unknown"), Lang::Fr);
    }

    #[test]
    fn test_weather_day_labels() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            assert!(!weather_day_label(lang, 0, true, false).is_empty());
            assert!(!weather_day_label(lang, 0, false, true).is_empty());
            for day in 0..7 {
                assert!(!weather_day_label(lang, day, false, false).is_empty());
            }
        }
    }

    #[test]
    fn test_gnews_status_labels() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            assert!(!gnews_status_label(lang, GNewsStatus::EmptyKey).is_empty());
            assert!(!gnews_status_label(lang, GNewsStatus::InvalidKey).is_empty());
            assert!(!gnews_status_label(lang, GNewsStatus::RateLimited).is_empty());
            assert!(!gnews_status_label(lang, GNewsStatus::NetworkError).is_empty());
            assert!(!gnews_status_label(lang, GNewsStatus::Loading).is_empty());
        }
    }

    #[test]
    fn test_word_clock_no_digits() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            for h in 0..24 {
                for m in (0..=55).step_by(5) {
                    let lines = word_clock_lines(lang, h, m);
                    assert!(!lines.is_empty());
                    for line in &lines {
                        assert!(
                            !line.chars().any(|c| c.is_ascii_digit()),
                            "Found digit in WordClock line '{}' for h={}, m={}",
                            line,
                            h,
                            m
                        );
                    }
                }
            }
        }
    }
}
