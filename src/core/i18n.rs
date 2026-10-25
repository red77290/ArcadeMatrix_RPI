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
    let is_partly = lower.contains("few clouds")
        || lower.contains("scattered")
        || lower.contains("peu nuageux")
        || lower.contains("partiellement")
        || lower.contains("algo de nubes")
        || lower.contains("nubes dispersas");
    let is_overcast = lower.contains("overcast")
        || lower.contains("couvert")
        || lower.contains("muy nuboso")
        || lower.contains("cubierto");
    match lang {
        Lang::Fr => {
            if lower.contains("clear") || lower.contains("sun") {
                "Soleil"
            } else if is_partly {
                "Eclairc."
            } else if is_overcast {
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
            } else if is_partly {
                "Parcial"
            } else if is_overcast {
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
            } else if is_partly {
                "P.Cloudy"
            } else if is_overcast {
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

// ----------------------------------------------------------------------------
// 7. Long Weather Labels (wide layouts)
// ----------------------------------------------------------------------------
/// Unabbreviated day label for wide weather layouts (same table as the ESP32
/// `I18n::getWeatherDayLabelLong`).
pub fn weather_day_label_long(
    lang: Lang,
    day_of_week: usize,
    is_today: bool,
    is_tomorrow: bool,
) -> &'static str {
    if is_today {
        return match lang {
            Lang::En => "TODAY",
            Lang::Es => "HOY",
            Lang::Fr => "AUJOURD'HUI",
        };
    }
    if is_tomorrow {
        return match lang {
            Lang::En => "TOMORROW",
            Lang::Es => "MANANA",
            Lang::Fr => "DEMAIN",
        };
    }
    const EN: [&str; 7] = [
        "SUNDAY",
        "MONDAY",
        "TUESDAY",
        "WEDNESDAY",
        "THURSDAY",
        "FRIDAY",
        "SATURDAY",
    ];
    const ES: [&str; 7] = [
        "DOMINGO",
        "LUNES",
        "MARTES",
        "MIERCOLES",
        "JUEVES",
        "VIERNES",
        "SABADO",
    ];
    const FR: [&str; 7] = [
        "DIMANCHE", "LUNDI", "MARDI", "MERCREDI", "JEUDI", "VENDREDI", "SAMEDI",
    ];
    match lang {
        Lang::En => EN[day_of_week % 7],
        Lang::Es => ES[day_of_week % 7],
        Lang::Fr => FR[day_of_week % 7],
    }
}

/// Unabbreviated OpenWeatherMap condition for wide weather layouts (same table
/// as the ESP32 `I18n::getWeatherConditionLong`).
pub fn weather_condition_long(lang: Lang, raw_condition: &str) -> &'static str {
    let l = raw_condition.trim().to_lowercase();
    let has = |k: &str| l.contains(k);
    let idx = if has("clear") || has("sun") {
        0
    } else if has("few clouds")
        || has("scattered")
        || has("peu nuageux")
        || has("partiellement")
        || has("algo de nubes")
        || has("nubes dispersas")
    {
        1
    } else if has("overcast") || has("couvert") || has("muy nuboso") || has("cubierto") {
        2
    } else if has("cloud") {
        3
    } else if has("thunder") || has("storm") {
        4
    } else if has("drizzle") {
        5
    } else if has("rain") {
        6
    } else if has("snow") {
        7
    } else if has("mist") {
        8
    } else if has("fog") {
        9
    } else {
        10
    };
    const EN: [&str; 11] = [
        "Clear",
        "Partly Cloudy",
        "Overcast",
        "Cloudy",
        "Thunderstorm",
        "Drizzle",
        "Rain",
        "Snow",
        "Mist",
        "Fog",
        "Clear",
    ];
    const ES: [&str; 11] = [
        "Soleado",
        "Parcialmente Nublado",
        "Cubierto",
        "Nublado",
        "Tormenta",
        "Llovizna",
        "Lluvia",
        "Nieve",
        "Bruma",
        "Niebla",
        "Variable",
    ];
    const FR: [&str; 11] = [
        "Soleil",
        "Eclaircies",
        "Couvert",
        "Nuageux",
        "Orage",
        "Bruine",
        "Pluie",
        "Neige",
        "Brume",
        "Brouillard",
        "Variable",
    ];
    match lang {
        Lang::En => EN[idx],
        Lang::Es => ES[idx],
        Lang::Fr => FR[idx],
    }
}

// ----------------------------------------------------------------------------
// 8. MQTT Data Engine
// ----------------------------------------------------------------------------
/// Shown for an MQTT Data page whose topic has no payload.
pub fn no_data_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "PAS DE DONNEES",
        Lang::En => "NO DATA",
        Lang::Es => "SIN DATOS",
    }
}

/// Shown while the MQTT Data engine connects to its broker.
pub fn connecting_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "CONNEXION",
        Lang::En => "CONNECTING",
        Lang::Es => "CONECTANDO",
    }
}

/// Shown when the MQTT Data engine can't reach its broker.
pub fn no_connection_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "PAS DE CONNEXION",
        Lang::En => "NO CONNECTION",
        Lang::Es => "SIN CONEXION",
    }
}

/// Shown for an MQTT Data page whose payload has a missing or unknown `type`
/// (or isn't JSON).
pub fn unsupported_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "NON PRIS EN CHARGE",
        Lang::En => "UNSUPPORTED",
        Lang::Es => "NO COMPATIBLE",
    }
}

/// Day label of the MQTT Data weather "NOW" page (live reading).
pub fn weather_now_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "ACTU.",
        Lang::En => "NOW",
        Lang::Es => "AHORA",
    }
}

/// Short / long display labels for the MQTT Data weather conditions
/// (the condition names listed in docs/MQTT_DATA_CONTRACT.md). `None` for an
/// unknown condition, which the caller shows as received.
pub fn weather_condition_labels(
    lang: Lang,
    condition: &str,
) -> Option<(&'static str, &'static str)> {
    let c = condition.trim().to_ascii_lowercase();
    let labels = match (c.as_str(), lang) {
        ("sunny", Lang::En) => ("Sunny", "Sunny"),
        ("sunny", Lang::Fr) => ("Soleil", "Ensoleillé"),
        ("sunny", Lang::Es) => ("Sol", "Soleado"),
        ("clear-night", Lang::En) => ("Clear", "Clear"),
        ("clear-night", Lang::Fr) => ("Clair", "Dégagé"),
        ("clear-night", Lang::Es) => ("Despejado", "Despejado"),
        ("partlycloudy", Lang::En) => ("Pt cloudy", "Partly cloudy"),
        ("partlycloudy", Lang::Fr) => ("Nuageux", "Partiellement nuageux"),
        ("partlycloudy", Lang::Es) => ("Nubes", "Parcialmente nublado"),
        ("cloudy", Lang::En) => ("Cloudy", "Cloudy"),
        ("cloudy", Lang::Fr) => ("Couvert", "Couvert"),
        ("cloudy", Lang::Es) => ("Nublado", "Nublado"),
        ("fog", Lang::En) => ("Fog", "Fog"),
        ("fog", Lang::Fr) => ("Brouil.", "Brouillard"),
        ("fog", Lang::Es) => ("Niebla", "Niebla"),
        ("exceptional", Lang::En) => ("Unusual", "Unusual"),
        ("exceptional", Lang::Fr) => ("Except.", "Exceptionnel"),
        ("exceptional", Lang::Es) => ("Inusual", "Inusual"),
        ("rainy", Lang::En) => ("Rain", "Rain"),
        ("rainy", Lang::Fr) => ("Pluie", "Pluie"),
        ("rainy", Lang::Es) => ("Lluvia", "Lluvia"),
        ("pouring", Lang::En) => ("Hvy rain", "Heavy rain"),
        ("pouring", Lang::Fr) => ("Averse", "Forte pluie"),
        ("pouring", Lang::Es) => ("Aguacero", "Lluvia fuerte"),
        ("lightning", Lang::En) => ("Storm", "Storm"),
        ("lightning", Lang::Fr) => ("Orage", "Orage"),
        ("lightning", Lang::Es) => ("Tormenta", "Tormenta"),
        ("lightning-rainy", Lang::En) => ("Storm+rain", "Storm + rain"),
        ("lightning-rainy", Lang::Fr) => ("Orage+pl.", "Orage et pluie"),
        ("lightning-rainy", Lang::Es) => ("Torm.+lluv.", "Tormenta y lluvia"),
        ("snowy", Lang::En) => ("Snow", "Snow"),
        ("snowy", Lang::Fr) => ("Neige", "Neige"),
        ("snowy", Lang::Es) => ("Nieve", "Nieve"),
        ("snowy-rainy", Lang::En) => ("Sleet", "Sleet"),
        ("snowy-rainy", Lang::Fr) => ("Neige fond.", "Neige fondue"),
        ("snowy-rainy", Lang::Es) => ("Aguanieve", "Aguanieve"),
        ("hail", Lang::En) => ("Hail", "Hail"),
        ("hail", Lang::Fr) => ("Grêle", "Grêle"),
        ("hail", Lang::Es) => ("Granizo", "Granizo"),
        ("windy" | "windy-variant", Lang::En) => ("Windy", "Windy"),
        ("windy" | "windy-variant", Lang::Fr) => ("Venteux", "Venteux"),
        ("windy" | "windy-variant", Lang::Es) => ("Viento", "Ventoso"),
        _ => return None,
    };
    Some(labels)
}

/// OpenWeatherMap icon code for an MQTT Data weather condition, so the existing
/// weather icons are reused. Unknown conditions get "03d" (clouds).
pub fn weather_icon_code(condition: &str) -> &'static str {
    match condition.trim().to_ascii_lowercase().as_str() {
        "sunny" => "01d",
        "clear-night" => "01n",
        "partlycloudy" => "02d",
        "cloudy" => "04d",
        "fog" | "exceptional" => "50d",
        "rainy" => "10d",
        "pouring" => "09d",
        "lightning" | "lightning-rainy" => "11d",
        "snowy" | "snowy-rainy" | "hail" => "13d",
        "windy" | "windy-variant" => "03d",
        _ => "03d",
    }
}

// ----------------------------------------------------------------------------
// 9. WordsClock (cw-cf-0x02 spoken time & date line)
// ----------------------------------------------------------------------------
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

pub fn spell_minutes(m: u32, lang: &str) -> String {
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

// ----------------------------------------------------------------------------
// 10. GNews Short Category Labels & Generic Engine UI Strings
// ----------------------------------------------------------------------------
pub fn gnews_category_short(lang: Lang, category: &str) -> &'static str {
    let cat = category.to_lowercase();
    if cat.contains("tech") {
        "TECH"
    } else if cat.contains("sci") {
        "SCI"
    } else if cat.contains("sport") {
        "SPORT"
    } else if cat.contains("bus") || cat.contains("fin") || cat.contains("econ") {
        "BIZ"
    } else if cat.contains("world") || cat.contains("nation") {
        match lang {
            Lang::Fr => "MONDE",
            Lang::Es => "MUNDO",
            Lang::En => "WORLD",
        }
    } else if cat.contains("ent") || cat.contains("art") {
        match lang {
            Lang::Fr => "CULT",
            Lang::Es => "CULT",
            Lang::En => "ARTS",
        }
    } else if cat.contains("heal") {
        match lang {
            Lang::Fr => "SANTE",
            Lang::Es => "SALUD",
            Lang::En => "HLTH",
        }
    } else {
        match lang {
            Lang::Fr => "INFOS",
            Lang::Es => "INFO",
            Lang::En => "NEWS",
        }
    }
}

/// Lookup localized UI/engine string by language code and key.
pub fn t(lang_code: &str, key: &str) -> &'static str {
    let lang = match lang_code.trim().to_lowercase().as_str() {
        "fr" => Lang::Fr,
        "es" => Lang::Es,
        _ => Lang::En,
    };
    match (key, lang) {
        ("no_api_key", Lang::Fr) => "Cle API manquante",
        ("no_api_key", Lang::Es) => "Sin clave API",
        ("no_api_key", Lang::En) => "No API key",

        ("no_city", Lang::Fr) => "Aucune ville",
        ("no_city", Lang::Es) => "Sin ciudad",
        ("no_city", Lang::En) => "No city",

        ("loading", Lang::Fr) => "Chargement...",
        ("loading", Lang::Es) => "Cargando...",
        ("loading", Lang::En) => "Loading...",

        ("loading_chart", Lang::Fr) => "Chargement graph...",
        ("loading_chart", Lang::Es) => "Cargando grafico...",
        ("loading_chart", Lang::En) => "Loading chart...",

        ("ready_to_stream", Lang::Fr) => "Pret a diffuser",
        ("ready_to_stream", Lang::Es) => "Listo para transmitir",
        ("ready_to_stream", Lang::En) => "Ready to stream",

        ("no_music_playing", Lang::Fr) => "Aucune musique",
        ("no_music_playing", Lang::Es) => "Sin reproduccion",
        ("no_music_playing", Lang::En) => "No music playing",

        ("waiting_for_marquee", Lang::Fr) => "EN ATTENTE DE MARQUEE",
        ("waiting_for_marquee", Lang::Es) => "ESPERANDO MARQUESINA",
        ("waiting_for_marquee", Lang::En) => "WAITING FOR MARQUEE",

        ("showcase_warming", Lang::Fr) => "PREPARATION VIDEO...",
        ("showcase_warming", Lang::Es) => "PREPARANDO VIDEO...",
        ("showcase_warming", Lang::En) => "WARMING VIDEO...",

        _ => "",
    }
}

/// Localized status prompt for Media Showcase pixel-art waiting scenes.
pub fn showcase_waiting_label(
    lang_code: &str,
    domain_slug: &str,
    unconfigured: bool,
) -> &'static str {
    let lang = match lang_code.trim().to_lowercase().as_str() {
        "fr" => Lang::Fr,
        "es" => Lang::Es,
        _ => Lang::En,
    };
    if unconfigured {
        return match lang {
            Lang::Fr => "CONFIGURER URL SERVEUR",
            Lang::Es => "CONFIGURAR URL SERVIDOR",
            Lang::En => "CONFIGURE SERVER URL",
        };
    }
    match (domain_slug, lang) {
        ("games", Lang::Fr) => "ATTENTE JEU RETRO...",
        ("games", Lang::Es) => "ESPERANDO JUEGO...",
        ("games", Lang::En) => "WAITING FOR GAME...",

        ("movies", Lang::Fr) => "ATTENTE FILM...",
        ("movies", Lang::Es) => "ESPERANDO PELICULA...",
        ("movies", Lang::En) => "WAITING FOR MOVIE...",

        ("series", Lang::Fr) => "SYNCHRO SERIE...",
        ("series", Lang::Es) => "SINTONIZANDO SERIE...",
        ("series", Lang::En) => "TUNING SERIES...",

        ("anime", Lang::Fr) => "ATTENTE ANIME...",
        ("anime", Lang::Es) => "ESPERANDO ANIME...",
        ("anime", Lang::En) => "WAITING FOR ANIME...",

        (_, Lang::Fr) => "PREPARATION MEDIA...",
        (_, Lang::Es) => "PREPARANDO MEDIA...",
        (_, Lang::En) => "PREPARING MEDIA...",
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
