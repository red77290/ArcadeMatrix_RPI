//! Streaming RFC 5545 `.ics` (iCalendar) parser, binary cache format (`AGND`),
//! and semantic sprite resolver for `AgendaEngine`.
//!
//! Binary cache layout is 100% byte-identical with the ESP32 firmware:
//! - [`AgendaCacheHeader`]: 24 bytes (`#[repr(C, packed)]`, Little-Endian)
//! - [`AgendaEventPod`]: 96 bytes (`#[repr(C, packed)]`, Little-Endian, up to 12 records)

use crate::engines::clocks::sprite_bank::{rgb888_to_rgb565, SpriteId};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::io::{BufRead, Read, Write};
use std::path::Path;

pub const AGENDA_CACHE_MAGIC: u32 = 0x4147_4E44; // "AGND"
pub const AGENDA_CACHE_VERSION: u8 = 1;
pub const MAX_AGENDA_EVENTS: usize = 12;

pub const FLAG_ALL_DAY: u8 = 1 << 0;
pub const FLAG_RECURRING: u8 = 1 << 1;
pub const FLAG_SOURCE_SECOND: u8 = 1 << 2;

/// 24-byte binary cache header (`data/cache/agenda_<instance_id>.bin`).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgendaCacheHeader {
    pub magic: u32,
    pub version: u8,
    pub record_count: u8,
    pub reserved: u16,
    pub config_crc32: u32,
    pub last_fetch_epoch: u32,
    pub payload_crc32: u32,
    pub schema_flags: u32,
}

impl Default for AgendaCacheHeader {
    fn default() -> Self {
        Self {
            magic: AGENDA_CACHE_MAGIC,
            version: AGENDA_CACHE_VERSION,
            record_count: 0,
            reserved: 0,
            config_crc32: 0,
            last_fetch_epoch: 0,
            payload_crc32: 0,
            schema_flags: 0,
        }
    }
}

impl AgendaCacheHeader {
    pub const SIZE: usize = 24;

    pub fn to_le_bytes(&self) -> [u8; 24] {
        let mut out = [0u8; 24];
        let magic = self.magic;
        let version = self.version;
        let record_count = self.record_count;
        let reserved = self.reserved;
        let config_crc32 = self.config_crc32;
        let last_fetch_epoch = self.last_fetch_epoch;
        let payload_crc32 = self.payload_crc32;
        let schema_flags = self.schema_flags;

        out[0..4].copy_from_slice(&magic.to_le_bytes());
        out[4] = version;
        out[5] = record_count;
        out[6..8].copy_from_slice(&reserved.to_le_bytes());
        out[8..12].copy_from_slice(&config_crc32.to_le_bytes());
        out[12..16].copy_from_slice(&last_fetch_epoch.to_le_bytes());
        out[16..20].copy_from_slice(&payload_crc32.to_le_bytes());
        out[20..24].copy_from_slice(&schema_flags.to_le_bytes());
        out
    }

    pub fn from_le_bytes(b: &[u8; 24]) -> Self {
        Self {
            magic: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            version: b[4],
            record_count: b[5],
            reserved: u16::from_le_bytes([b[6], b[7]]),
            config_crc32: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            last_fetch_epoch: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            payload_crc32: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
            schema_flags: u32::from_le_bytes([b[20], b[21], b[22], b[23]]),
        }
    }
}

/// 96-byte Plain-Old-Data event record stored in the binary cache and active engine buffer.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgendaEventPod {
    pub start_epoch: u32,
    pub end_epoch: u32,
    pub calendar_color_565: u16,
    pub flags: u8,
    pub sprite_id: u8,
    pub title: [u8; 64],
    pub badge_label: [u8; 20],
}

impl Default for AgendaEventPod {
    fn default() -> Self {
        Self::empty()
    }
}

impl AgendaEventPod {
    pub const SIZE: usize = 96;

    pub const fn empty() -> Self {
        Self {
            start_epoch: 0,
            end_epoch: 0,
            calendar_color_565: 0,
            flags: 0,
            sprite_id: 0,
            title: [0u8; 64],
            badge_label: [0u8; 20],
        }
    }

    #[inline]
    pub const fn is_all_day(&self) -> bool {
        (self.flags & FLAG_ALL_DAY) != 0
    }

    #[inline]
    pub const fn is_recurring(&self) -> bool {
        (self.flags & FLAG_RECURRING) != 0
    }

    #[inline]
    pub const fn source_index(&self) -> u8 {
        if (self.flags & FLAG_SOURCE_SECOND) != 0 {
            1
        } else {
            0
        }
    }

    #[inline]
    pub fn title_str(&self) -> &str {
        let len = self
            .title
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.title.len());
        core::str::from_utf8(&self.title[..len]).unwrap_or("")
    }

    #[inline]
    pub fn badge_label_str(&self) -> &str {
        let len = self
            .badge_label
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.badge_label.len());
        core::str::from_utf8(&self.badge_label[..len]).unwrap_or("")
    }

    #[inline]
    pub fn set_title(&mut self, s: &str) {
        copy_utf8_truncated(s, &mut self.title);
    }

    #[inline]
    pub fn set_badge_label(&mut self, s: &str) {
        copy_utf8_truncated(s, &mut self.badge_label);
    }

    pub fn to_le_bytes(&self) -> [u8; 96] {
        let mut out = [0u8; 96];
        let start_epoch = self.start_epoch;
        let end_epoch = self.end_epoch;
        let calendar_color_565 = self.calendar_color_565;
        out[0..4].copy_from_slice(&start_epoch.to_le_bytes());
        out[4..8].copy_from_slice(&end_epoch.to_le_bytes());
        out[8..10].copy_from_slice(&calendar_color_565.to_le_bytes());
        out[10] = self.flags;
        out[11] = self.sprite_id;
        out[12..76].copy_from_slice(&self.title);
        out[76..96].copy_from_slice(&self.badge_label);
        out
    }

    pub fn from_le_bytes(b: &[u8; 96]) -> Self {
        let mut title = [0u8; 64];
        title.copy_from_slice(&b[12..76]);
        let mut badge_label = [0u8; 20];
        badge_label.copy_from_slice(&b[76..96]);
        Self {
            start_epoch: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            end_epoch: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            calendar_color_565: u16::from_le_bytes([b[8], b[9]]),
            flags: b[10],
            sprite_id: b[11],
            title,
            badge_label,
        }
    }
}

/// Standard IEEE 802.3 CRC32 (matching ESP32 `esp_crc32_le`).
pub fn crc32_bytes(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Computes `config_crc32` over `"{ics_url_1}|{ics_url_2}|{horizon_days}"`.
pub fn compute_agenda_config_crc32(ics_url_1: &str, ics_url_2: &str, horizon_days: i32) -> u32 {
    let mut buf = Vec::with_capacity(ics_url_1.len() + ics_url_2.len() + 16);
    buf.extend_from_slice(ics_url_1.trim().as_bytes());
    buf.push(b'|');
    buf.extend_from_slice(ics_url_2.trim().as_bytes());
    buf.push(b'|');
    buf.extend_from_slice(horizon_days.to_string().as_bytes());
    crc32_bytes(&buf)
}

/// Parses `#RRGGBB` hex color into RGB565.
pub fn parse_hex_color_565(hex: &str, default_rgb: (u8, u8, u8)) -> u16 {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        ) {
            return rgb888_to_rgb565(r, g, b);
        }
    }
    rgb888_to_rgb565(default_rgb.0, default_rgb.1, default_rgb.2)
}

/// Normalizes `webcal://` or `webcals://` URLs to `https://`.
pub fn normalize_ics_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.len() >= 9 && trimmed[..9].eq_ignore_ascii_case("webcal://") {
        format!("https://{}", &trimmed[9..])
    } else if trimmed.len() >= 10 && trimmed[..10].eq_ignore_ascii_case("webcals://") {
        format!("https://{}", &trimmed[10..])
    } else {
        trimmed.to_string()
    }
}

/// Unescapes RFC 5545 text escapes (`\n`, `\N`, `\,`, `\;`, `\\`) and normalizes UTF-8
/// characters into clean displayable text without cutting UTF-8 codepoints.
pub fn unescape_ics_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some('n') | Some('N') => {
                    let _ = chars.next();
                    out.push(' ');
                }
                Some(',') => {
                    let _ = chars.next();
                    out.push(',');
                }
                Some(';') => {
                    let _ = chars.next();
                    out.push(';');
                }
                Some('\\') => {
                    let _ = chars.next();
                    out.push('\\');
                }
                Some(other) => {
                    let _ = chars.next();
                    out.push(other);
                }
                None => {}
            }
        } else if c == '\r' || c == '\n' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Copies a UTF-8 string into a fixed byte buffer without splitting multi-byte UTF-8 characters.
pub fn copy_utf8_truncated(src: &str, dst: &mut [u8]) {
    dst.fill(0);
    if dst.is_empty() {
        return;
    }
    let max_bytes = dst.len() - 1; // keep NUL terminatorroom or full slice
    let bytes = src.as_bytes();
    if bytes.len() <= max_bytes {
        dst[..bytes.len()].copy_from_slice(bytes);
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !src.is_char_boundary(end) {
        end -= 1;
    }
    dst[..end].copy_from_slice(&bytes[..end]);
}

/// Resolves semantic `SpriteId` from an event summary/title (`AgendaSpriteResolver`).
pub fn resolve_agenda_sprite(summary: &str) -> SpriteId {
    let lower = summary.to_lowercase();
    let has_word_or_sub = |kw: &str| lower.contains(kw);

    // 1. Birthday
    if has_word_or_sub("anniv")
        || has_word_or_sub("birthday")
        || has_word_or_sub("cumple")
        || summary.contains('🎂')
        || summary.contains('🎉')
    {
        return SpriteId::BirthdayCake;
    }

    // 2. Call / Meeting
    if has_word_or_sub("call")
        || has_word_or_sub("reunion")
        || has_word_or_sub("réunion")
        || has_word_or_sub("meeting")
        || has_word_or_sub("teams")
        || has_word_or_sub("zoom")
        || has_word_or_sub("daily")
        || has_word_or_sub("standup")
        || summary.contains('💻')
    {
        return SpriteId::CyberCall;
    }

    // 3. Medical
    if has_word_or_sub("medecin")
        || has_word_or_sub("médecin")
        || has_word_or_sub("dentiste")
        || has_word_or_sub("docteur")
        || has_word_or_sub("rdv medical")
        || has_word_or_sub("veto")
        || has_word_or_sub("doctor")
        || summary.contains('🩺')
    {
        return SpriteId::MedicalHeart;
    }

    // 4. Food / Resto
    if has_word_or_sub("resto")
        || has_word_or_sub("diner")
        || has_word_or_sub("dîner")
        || has_word_or_sub("dej")
        || has_word_or_sub("apéro")
        || has_word_or_sub("apero")
        || has_word_or_sub("pizza")
        || has_word_or_sub("burger")
        || has_word_or_sub("lunch")
        || summary.contains('🍕')
        || summary.contains('🍻')
    {
        return SpriteId::FoodPizza;
    }

    // 5. Sport
    if has_word_or_sub("sport")
        || has_word_or_sub("foot")
        || has_word_or_sub("tennis")
        || has_word_or_sub("padel")
        || has_word_or_sub("salle")
        || has_word_or_sub("gym")
        || has_word_or_sub("run")
        || summary.contains('⚽')
    {
        return SpriteId::SportSonic;
    }

    // 6. Travel
    if has_word_or_sub("train")
        || has_word_or_sub("sncf")
        || has_word_or_sub("vol")
        || has_word_or_sub("avion")
        || has_word_or_sub("vacances")
        || has_word_or_sub("flight")
        || summary.contains('✈')
        || summary.contains('🚆')
    {
        return SpriteId::TravelPlane;
    }

    // 7. Gaming
    if has_word_or_sub("jeu")
        || has_word_or_sub("arcade")
        || has_word_or_sub("lan")
        || has_word_or_sub("gaming")
        || has_word_or_sub("stream")
        || summary.contains('🎮')
    {
        return SpriteId::GamingPacman;
    }

    SpriteId::CalendarDefault
}

/// Reads ONLY the 24-byte `AgendaCacheHeader` from disk (`initialize()` fast path).
pub fn read_cache_header_only(path: &Path) -> Option<AgendaCacheHeader> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 24];
    file.read_exact(&mut buf).ok()?;
    let hdr = AgendaCacheHeader::from_le_bytes(&buf);
    if hdr.magic != AGENDA_CACHE_MAGIC
        || hdr.version != AGENDA_CACHE_VERSION
        || (hdr.record_count as usize) > MAX_AGENDA_EVENTS
    {
        return None;
    }
    Some(hdr)
}

/// Reads and validates the full binary cache (`AgendaCacheHeader` + `AgendaEventPod` payload CRC32).
pub fn read_cache_records(
    path: &Path,
    expected_config_crc32: u32,
) -> Option<(
    AgendaCacheHeader,
    [AgendaEventPod; MAX_AGENDA_EVENTS],
    usize,
)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < AgendaCacheHeader::SIZE {
        return None;
    }
    let mut hdr_bytes = [0u8; 24];
    hdr_bytes.copy_from_slice(&bytes[0..24]);
    let hdr = AgendaCacheHeader::from_le_bytes(&hdr_bytes);
    if hdr.magic != AGENDA_CACHE_MAGIC
        || hdr.version != AGENDA_CACHE_VERSION
        || hdr.config_crc32 != expected_config_crc32
    {
        return None;
    }
    let count = (hdr.record_count as usize).min(MAX_AGENDA_EVENTS);
    let expected_payload_len = count * AgendaEventPod::SIZE;
    if bytes.len() < AgendaCacheHeader::SIZE + expected_payload_len {
        return None;
    }
    let payload_slice = &bytes[24..24 + expected_payload_len];
    if crc32_bytes(payload_slice) != hdr.payload_crc32 {
        return None;
    }
    let mut pods = [AgendaEventPod::empty(); MAX_AGENDA_EVENTS];
    for i in 0..count {
        let mut rec_bytes = [0u8; 96];
        let off = i * 96;
        rec_bytes.copy_from_slice(&payload_slice[off..off + 96]);
        pods[i] = AgendaEventPod::from_le_bytes(&rec_bytes);
    }
    Some((hdr, pods, count))
}

/// Atomically writes `AgendaCacheHeader` + `records[..count]` via `.bin.tmp` -> `rename`.
pub fn write_cache_records_atomic(
    path: &Path,
    config_crc32: u32,
    fetch_epoch: u32,
    records: &[AgendaEventPod],
) -> bool {
    let count = records.len().min(MAX_AGENDA_EVENTS);
    let mut payload = Vec::with_capacity(count * AgendaEventPod::SIZE);
    for rec in records.iter().take(count) {
        payload.extend_from_slice(&rec.to_le_bytes());
    }
    let payload_crc = crc32_bytes(&payload);
    let hdr = AgendaCacheHeader {
        magic: AGENDA_CACHE_MAGIC,
        version: AGENDA_CACHE_VERSION,
        record_count: count as u8,
        reserved: 0,
        config_crc32,
        last_fetch_epoch: fetch_epoch,
        payload_crc32: payload_crc,
        schema_flags: 0,
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp_path = path.with_extension("bin.tmp");
    let write_ok = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(&hdr.to_le_bytes())?;
        f.write_all(&payload)?;
        f.sync_all()?;
        Ok(())
    })()
    .is_ok();

    if !write_ok {
        let _ = std::fs::remove_file(&tmp_path);
        return false;
    }
    if std::fs::rename(&tmp_path, path).is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        return false;
    }
    true
}

/// Insertion-sorts an `AgendaEventPod` into a bounded array `[AgendaEventPod; MAX_AGENDA_EVENTS]`
/// ordered by `(start_epoch, end_epoch)`. Deduplicates identical `(start_epoch, title)`.
pub fn insert_sorted_bounded(
    buf: &mut [AgendaEventPod; MAX_AGENDA_EVENTS],
    count: &mut usize,
    pod: AgendaEventPod,
) {
    let pod_start = pod.start_epoch;
    let pod_end = pod.end_epoch;
    for existing in buf.iter().take(*count) {
        let ex_start = existing.start_epoch;
        if ex_start == pod_start && existing.title == pod.title {
            return;
        }
    }

    let mut insert_idx = *count;
    for i in 0..*count {
        let ex_start = buf[i].start_epoch;
        let ex_end = buf[i].end_epoch;
        if pod_start < ex_start || (pod_start == ex_start && pod_end < ex_end) {
            insert_idx = i;
            break;
        }
    }

    if insert_idx >= MAX_AGENDA_EVENTS {
        return;
    }

    let new_count = (*count + 1).min(MAX_AGENDA_EVENTS);
    for j in (insert_idx + 1..new_count).rev() {
        buf[j] = buf[j - 1];
    }
    buf[insert_idx] = pod;
    *count = new_count;
}

#[derive(Clone, Debug, Default)]
struct RawVEvent {
    dtstart_raw: String,
    dtstart_params: String,
    dtend_raw: String,
    dtend_params: String,
    duration_raw: String,
    summary: String,
    location: String,
    status_cancelled: bool,
    rrule: String,
    exdates: Vec<u32>,
}

fn map_windows_or_posix_tz(tz_str: &str) -> Option<Tz> {
    let trimmed = tz_str.trim().trim_matches('"');
    if let Ok(tz) = trimmed.parse::<Tz>() {
        return Some(tz);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("romance")
        || lower.contains("central europe")
        || lower.contains("w. europe")
        || lower.starts_with("cet")
    {
        return Some(chrono_tz::Europe::Paris);
    }
    if lower.contains("gmt") || lower.contains("utc") {
        return Some(chrono_tz::UTC);
    }
    if lower.contains("eastern") || lower.starts_with("est") {
        return Some(chrono_tz::America::New_York);
    }
    if lower.contains("pacific") || lower.starts_with("pst") {
        return Some(chrono_tz::America::Los_Angeles);
    }
    None
}

/// Parses an RFC 5545 date or date-time value into `(epoch_sec, is_all_day, NaiveDateTime)`.
pub fn parse_ics_datetime(
    val_raw: &str,
    params_raw: &str,
    default_tz_str: &str,
) -> Option<(u32, bool, NaiveDateTime)> {
    let val = val_raw.trim();
    if val.len() < 8 {
        return None;
    }
    let year: i32 = val.get(0..4)?.parse().ok()?;
    let month: u32 = val.get(4..6)?.parse().ok()?;
    let day: u32 = val.get(6..8)?.parse().ok()?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;

    let params_upper = params_raw.to_ascii_uppercase();
    let is_date_only = params_upper.contains("VALUE=DATE") && !params_upper.contains("DATE-TIME")
        || !val.contains('T');

    // Extract TZID if present
    let mut tzid_opt: Option<Tz> = None;
    for part in params_raw.split(';') {
        if let Some((k, v)) = part.split_once('=') {
            if k.trim().eq_ignore_ascii_case("TZID") {
                tzid_opt = map_windows_or_posix_tz(v);
            }
        }
    }
    let fallback_tz = tzid_opt
        .or_else(|| map_windows_or_posix_tz(default_tz_str))
        .unwrap_or(chrono_tz::Europe::Paris);

    if is_date_only {
        let ndt = date.and_hms_opt(0, 0, 0)?;
        let epoch = fallback_tz
            .from_local_datetime(&ndt)
            .earliest()
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| ndt.and_utc().timestamp());
        return Some((epoch.max(0) as u32, true, ndt));
    }

    let t_pos = val.find('T')?;
    let time_part = &val[t_pos + 1..];
    if time_part.len() < 4 {
        return None;
    }
    let hour: u32 = time_part.get(0..2)?.parse().ok()?;
    let min: u32 = time_part.get(2..4)?.parse().ok()?;
    let sec: u32 = if time_part.len() >= 6 && time_part.as_bytes()[4].is_ascii_digit() {
        time_part
            .get(4..6)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    let time = NaiveTime::from_hms_opt(hour, min, sec)?;
    let ndt = NaiveDateTime::new(date, time);

    let is_utc = time_part.ends_with('Z') || time_part.ends_with('z');
    let epoch = if is_utc {
        Utc.from_utc_datetime(&ndt).timestamp()
    } else {
        fallback_tz
            .from_local_datetime(&ndt)
            .earliest()
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| ndt.and_utc().timestamp())
    };
    Some((epoch.max(0) as u32, false, ndt))
}

fn local_ndt_to_epoch(ndt: NaiveDateTime, is_utc: bool, tz: Tz) -> u32 {
    let ts = if is_utc {
        Utc.from_utc_datetime(&ndt).timestamp()
    } else {
        tz.from_local_datetime(&ndt)
            .earliest()
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| ndt.and_utc().timestamp())
    };
    ts.max(0) as u32
}

/// Parses RFC 5545 `DURATION` (e.g. `PT1H30M`, `P1D`, `PT45M`) into seconds.
pub fn parse_ics_duration_secs(raw: &str) -> u32 {
    let s = raw.trim().to_ascii_uppercase();
    if !s.starts_with('P') {
        return 0;
    }
    let mut total: u32 = 0;
    let mut num: u32 = 0;
    let mut in_time = false;
    for ch in s[1..].chars() {
        if ch == 'T' {
            in_time = true;
            num = 0;
        } else if let Some(d) = ch.to_digit(10) {
            num = num.saturating_mul(10).saturating_add(d);
        } else {
            match (in_time, ch) {
                (false, 'W') => total = total.saturating_add(num.saturating_mul(7 * 86400)),
                (false, 'D') => total = total.saturating_add(num.saturating_mul(86400)),
                (true, 'H') => total = total.saturating_add(num.saturating_mul(3600)),
                (true, 'M') => total = total.saturating_add(num.saturating_mul(60)),
                (true, 'S') => total = total.saturating_add(num),
                _ => {}
            }
            num = 0;
        }
    }
    total
}

fn parse_weekday_token(tok: &str) -> Option<u32> {
    let letters: String = tok
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    match letters.as_str() {
        "SU" => Some(0),
        "MO" => Some(1),
        "TU" => Some(2),
        "WE" => Some(3),
        "TH" => Some(4),
        "FR" => Some(5),
        "SA" => Some(6),
        _ => None,
    }
}

fn is_exdate_excluded(cand_start: u32, exdates: &[u32], is_all_day: bool) -> bool {
    for &ex in exdates {
        if ex == cand_start {
            return true;
        }
        // Also match same calendar day if EXDATE was specified as a date or slightly shifted DST
        if is_all_day && ex.abs_diff(cand_start) < 43200 {
            return true;
        }
        if ex.abs_diff(cand_start) <= 3600 && (ex.abs_diff(cand_start)).is_multiple_of(3600) {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn evaluate_vevent_into_bounded(
    ev: &RawVEvent,
    now_epoch: u32,
    horizon_end_epoch: u32,
    default_tz_str: &str,
    source_idx: u8,
    calendar_color_565: u16,
    badge_label: &str,
    out_pods: &mut [AgendaEventPod; MAX_AGENDA_EVENTS],
    out_count: &mut usize,
) {
    if ev.status_cancelled || ev.dtstart_raw.is_empty() {
        return;
    }

    let Some((base_start, is_all_day, base_ndt)) =
        parse_ics_datetime(&ev.dtstart_raw, &ev.dtstart_params, default_tz_str)
    else {
        return;
    };

    let duration_secs = if !ev.dtend_raw.is_empty() {
        if let Some((end_ep, _, _)) =
            parse_ics_datetime(&ev.dtend_raw, &ev.dtend_params, default_tz_str)
        {
            end_ep
                .saturating_sub(base_start)
                .max(if is_all_day { 86400 } else { 60 })
        } else {
            if is_all_day {
                86400
            } else {
                3600
            }
        }
    } else if !ev.duration_raw.is_empty() {
        parse_ics_duration_secs(&ev.duration_raw).max(if is_all_day { 86400 } else { 60 })
    } else if is_all_day {
        86400
    } else {
        3600
    };

    let clean_title = unescape_ics_text(if ev.summary.trim().is_empty() {
        "Event"
    } else {
        &ev.summary
    });
    let sprite_id = resolve_agenda_sprite(&clean_title) as u8;
    let mut base_flags = if is_all_day { FLAG_ALL_DAY } else { 0 };
    if source_idx > 0 {
        base_flags |= FLAG_SOURCE_SECOND;
    }
    if !ev.rrule.is_empty() {
        base_flags |= FLAG_RECURRING;
    }

    let make_pod = |start_ep: u32, end_ep: u32| -> AgendaEventPod {
        let mut pod = AgendaEventPod {
            start_epoch: start_ep,
            end_epoch: end_ep,
            calendar_color_565,
            flags: base_flags,
            sprite_id,
            title: [0u8; 64],
            badge_label: [0u8; 20],
        };
        copy_utf8_truncated(&clean_title, &mut pod.title);
        copy_utf8_truncated(badge_label.trim(), &mut pod.badge_label);
        pod
    };

    if ev.rrule.trim().is_empty() {
        let base_end = base_start.saturating_add(duration_secs);
        if base_end > now_epoch
            && base_start <= horizon_end_epoch
            && !is_exdate_excluded(base_start, &ev.exdates, is_all_day)
        {
            insert_sorted_bounded(out_pods, out_count, make_pod(base_start, base_end));
        }
        return;
    }

    // Parse RRULE parameters
    let mut freq = "";
    let mut interval: u32 = 1;
    let mut until_epoch: Option<u32> = None;
    let mut count_limit: Option<u32> = None;
    let mut byday_mask: u8 = 0;

    for part in ev.rrule.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        if key.eq_ignore_ascii_case("FREQ") {
            freq = val;
        } else if key.eq_ignore_ascii_case("INTERVAL") {
            interval = val.parse::<u32>().unwrap_or(1).max(1);
        } else if key.eq_ignore_ascii_case("COUNT") {
            count_limit = val.parse::<u32>().ok();
        } else if key.eq_ignore_ascii_case("UNTIL") {
            if let Some((u_ep, u_all_day, _)) = parse_ics_datetime(val, "", default_tz_str) {
                until_epoch = Some(if u_all_day {
                    u_ep.saturating_add(86399)
                } else {
                    u_ep
                });
            }
        } else if key.eq_ignore_ascii_case("BYDAY") {
            for wd_tok in val.split(',') {
                if let Some(wd_idx) = parse_weekday_token(wd_tok) {
                    byday_mask |= 1 << wd_idx;
                }
            }
        }
    }

    let mut tzid_opt: Option<Tz> = None;
    for part in ev.dtstart_params.split(';') {
        if let Some((k, v)) = part.split_once('=') {
            if k.trim().eq_ignore_ascii_case("TZID") {
                tzid_opt = map_windows_or_posix_tz(v);
            }
        }
    }
    let tz = tzid_opt
        .or_else(|| map_windows_or_posix_tz(default_tz_str))
        .unwrap_or(chrono_tz::Europe::Paris);
    let is_utc = ev.dtstart_raw.trim().ends_with('Z') && !is_all_day;

    let mut occurrences: u32 = 0;
    let max_iters = count_limit.unwrap_or(2048).min(2048);

    if freq.eq_ignore_ascii_case("YEARLY") {
        let mut step = 0u32;
        while occurrences < max_iters && step < 150 {
            let target_year = base_ndt.year() + (step * interval) as i32;
            step += 1;
            let Some(cand_date) =
                NaiveDate::from_ymd_opt(target_year, base_ndt.month(), base_ndt.day())
            else {
                continue;
            };
            let cand_ndt = NaiveDateTime::new(cand_date, base_ndt.time());
            let cand_start = local_ndt_to_epoch(cand_ndt, is_utc, tz);
            if let Some(until) = until_epoch {
                if cand_start > until {
                    break;
                }
            }
            occurrences += 1;
            if cand_start > horizon_end_epoch {
                break;
            }
            let cand_end = cand_start.saturating_add(duration_secs);
            if cand_end > now_epoch && !is_exdate_excluded(cand_start, &ev.exdates, is_all_day) {
                insert_sorted_bounded(out_pods, out_count, make_pod(cand_start, cand_end));
            }
        }
    } else if freq.eq_ignore_ascii_case("MONTHLY") {
        let mut step = 0u32;
        while occurrences < max_iters && step < 600 {
            let total_months =
                (base_ndt.year() * 12 + (base_ndt.month() as i32 - 1)) + (step * interval) as i32;
            step += 1;
            let y = total_months.div_euclid(12);
            let m = (total_months.rem_euclid(12) + 1) as u32;
            let Some(cand_date) = NaiveDate::from_ymd_opt(y, m, base_ndt.day()) else {
                continue;
            };
            let cand_ndt = NaiveDateTime::new(cand_date, base_ndt.time());
            let cand_start = local_ndt_to_epoch(cand_ndt, is_utc, tz);
            if let Some(until) = until_epoch {
                if cand_start > until {
                    break;
                }
            }
            occurrences += 1;
            if cand_start > horizon_end_epoch {
                break;
            }
            let cand_end = cand_start.saturating_add(duration_secs);
            if cand_end > now_epoch && !is_exdate_excluded(cand_start, &ev.exdates, is_all_day) {
                insert_sorted_bounded(out_pods, out_count, make_pod(cand_start, cand_end));
            }
        }
    } else if freq.eq_ignore_ascii_case("WEEKLY") {
        if byday_mask == 0 {
            let wd = base_ndt.weekday().num_days_from_sunday();
            byday_mask = 1 << wd;
        }
        let base_date = base_ndt.date();
        let mut day_offset: i64 = 0;
        // Fast-forward if event started far in the past and has no COUNT limit
        if count_limit.is_none() && base_start + 86400 * 14 < now_epoch {
            let diff_days = ((now_epoch - base_start) / 86400) as i64;
            let week_span = (7 * interval) as i64;
            let skip_weeks = (diff_days - 7).max(0) / week_span;
            day_offset = skip_weeks * week_span;
        }
        while occurrences < max_iters && day_offset < 3650 {
            let week_index = (day_offset / 7) as u32;
            if !week_index.is_multiple_of(interval) {
                day_offset += 1;
                continue;
            }
            let Some(cand_date) = base_date.checked_add_signed(Duration::days(day_offset)) else {
                break;
            };
            day_offset += 1;
            let wd = cand_date.weekday().num_days_from_sunday();
            if (byday_mask & (1 << wd)) == 0 {
                continue;
            }
            let cand_ndt = NaiveDateTime::new(cand_date, base_ndt.time());
            let cand_start = local_ndt_to_epoch(cand_ndt, is_utc, tz);
            if cand_start < base_start {
                continue;
            }
            if let Some(until) = until_epoch {
                if cand_start > until {
                    break;
                }
            }
            occurrences += 1;
            if cand_start > horizon_end_epoch {
                break;
            }
            let cand_end = cand_start.saturating_add(duration_secs);
            if cand_end > now_epoch && !is_exdate_excluded(cand_start, &ev.exdates, is_all_day) {
                insert_sorted_bounded(out_pods, out_count, make_pod(cand_start, cand_end));
            }
        }
    } else if freq.eq_ignore_ascii_case("DAILY") {
        let mut step: i64 = 0;
        if count_limit.is_none() && base_start + 86400 * 7 < now_epoch {
            let diff_days = ((now_epoch - base_start) / 86400) as i64;
            step = ((diff_days - 2).max(0)) / (interval as i64);
        }
        while occurrences < max_iters && step < 3650 {
            let day_delta = step * (interval as i64);
            step += 1;
            let Some(cand_date) = base_ndt
                .date()
                .checked_add_signed(Duration::days(day_delta))
            else {
                break;
            };
            let cand_ndt = NaiveDateTime::new(cand_date, base_ndt.time());
            let cand_start = local_ndt_to_epoch(cand_ndt, is_utc, tz);
            if let Some(until) = until_epoch {
                if cand_start > until {
                    break;
                }
            }
            occurrences += 1;
            if cand_start > horizon_end_epoch {
                break;
            }
            let cand_end = cand_start.saturating_add(duration_secs);
            if cand_end > now_epoch && !is_exdate_excluded(cand_start, &ev.exdates, is_all_day) {
                insert_sorted_bounded(out_pods, out_count, make_pod(cand_start, cand_end));
            }
        }
    }
}

fn process_logical_ics_line(
    line: &str,
    in_vevent: &mut bool,
    in_nested_alarm: &mut bool,
    current_ev: &mut RawVEvent,
    now_epoch: u32,
    horizon_end_epoch: u32,
    default_tz_str: &str,
    source_idx: u8,
    calendar_color_565: u16,
    badge_label: &str,
    out_pods: &mut [AgendaEventPod; MAX_AGENDA_EVENTS],
    out_count: &mut usize,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if trimmed.eq_ignore_ascii_case("BEGIN:VEVENT") {
        *in_vevent = true;
        *in_nested_alarm = false;
        *current_ev = RawVEvent::default();
        return;
    }
    if trimmed.eq_ignore_ascii_case("END:VEVENT") {
        if *in_vevent {
            evaluate_vevent_into_bounded(
                current_ev,
                now_epoch,
                horizon_end_epoch,
                default_tz_str,
                source_idx,
                calendar_color_565,
                badge_label,
                out_pods,
                out_count,
            );
        }
        *in_vevent = false;
        *in_nested_alarm = false;
        return;
    }
    if !*in_vevent {
        return;
    }
    if trimmed
        .as_bytes()
        .get(..12)
        .is_some_and(|b| b.eq_ignore_ascii_case(b"BEGIN:VALARM"))
    {
        *in_nested_alarm = true;
        return;
    }
    if trimmed
        .as_bytes()
        .get(..10)
        .is_some_and(|b| b.eq_ignore_ascii_case(b"END:VALARM"))
    {
        *in_nested_alarm = false;
        return;
    }
    if *in_nested_alarm {
        return;
    }

    let Some((prop_part, val_part)) = trimmed.split_once(':') else {
        return;
    };
    let (prop_name, prop_params) = match prop_part.split_once(';') {
        Some((n, p)) => (n.trim(), p.trim()),
        None => (prop_part.trim(), ""),
    };

    if prop_name.eq_ignore_ascii_case("DTSTART") {
        current_ev.dtstart_raw = val_part.trim().to_string();
        current_ev.dtstart_params = prop_params.to_string();
    } else if prop_name.eq_ignore_ascii_case("DTEND") {
        current_ev.dtend_raw = val_part.trim().to_string();
        current_ev.dtend_params = prop_params.to_string();
    } else if prop_name.eq_ignore_ascii_case("DURATION") {
        current_ev.duration_raw = val_part.trim().to_string();
    } else if prop_name.eq_ignore_ascii_case("SUMMARY") {
        current_ev.summary = val_part.trim().to_string();
    } else if prop_name.eq_ignore_ascii_case("LOCATION") {
        current_ev.location = val_part.trim().to_string();
    } else if prop_name.eq_ignore_ascii_case("STATUS") {
        if val_part.trim().eq_ignore_ascii_case("CANCELLED") {
            current_ev.status_cancelled = true;
        }
    } else if prop_name.eq_ignore_ascii_case("RRULE") {
        current_ev.rrule = val_part.trim().to_string();
    } else if prop_name.eq_ignore_ascii_case("EXDATE") {
        for item in val_part.split(',') {
            if let Some((ep, _, _)) = parse_ics_datetime(item, prop_params, default_tz_str) {
                current_ev.exdates.push(ep);
            }
        }
    }
}

/// Streams and parses an RFC 5545 `.ics` byte reader with byte-level line unfolding
/// (preserving multibyte UTF-8 characters split at byte 75) into `out_pods`.
#[allow(clippy::too_many_arguments)]
pub fn parse_ics_stream<R: Read>(
    reader: R,
    now_epoch: u32,
    horizon_days: u32,
    default_tz_str: &str,
    source_idx: u8,
    calendar_color_565: u16,
    badge_label: &str,
    out_pods: &mut [AgendaEventPod; MAX_AGENDA_EVENTS],
    out_count: &mut usize,
) {
    let horizon_end_epoch = now_epoch.saturating_add(horizon_days.clamp(1, 30) * 86400);
    let mut buf_reader = std::io::BufReader::with_capacity(2048, reader);
    let mut raw_line = Vec::with_capacity(256);
    let mut logical_line: Vec<u8> = Vec::with_capacity(512);

    let mut in_vevent = false;
    let mut in_nested_alarm = false;
    let mut current_ev = RawVEvent::default();

    loop {
        raw_line.clear();
        let Ok(n) = buf_reader.read_until(b'\n', &mut raw_line) else {
            break;
        };
        if n == 0 {
            break;
        }
        // Strip trailing \r\n or \n
        while matches!(raw_line.last(), Some(b'\n' | b'\r')) {
            raw_line.pop();
        }

        // RFC 5545 §3.1: Lines starting with SPACE (0x20) or HTAB (0x09) are continuations
        // of the previous logical line. Unfold at the raw byte level BEFORE UTF-8 decoding
        // so multibyte UTF-8 characters split at column 75 are reassembled intact!
        if matches!(raw_line.first(), Some(b' ' | b'\t')) {
            if logical_line.len() + raw_line.len() <= 2048 {
                logical_line.extend_from_slice(&raw_line[1..]);
            }
            continue;
        }

        if !logical_line.is_empty() {
            let decoded = String::from_utf8_lossy(&logical_line);
            process_logical_ics_line(
                &decoded,
                &mut in_vevent,
                &mut in_nested_alarm,
                &mut current_ev,
                now_epoch,
                horizon_end_epoch,
                default_tz_str,
                source_idx,
                calendar_color_565,
                badge_label,
                out_pods,
                out_count,
            );
            logical_line.clear();
        }

        logical_line.extend_from_slice(&raw_line);
    }

    if !logical_line.is_empty() {
        let decoded = String::from_utf8_lossy(&logical_line);
        process_logical_ics_line(
            &decoded,
            &mut in_vevent,
            &mut in_nested_alarm,
            &mut current_ev,
            now_epoch,
            horizon_end_epoch,
            default_tz_str,
            source_idx,
            calendar_color_565,
            badge_label,
            out_pods,
            out_count,
        );
    }
}
