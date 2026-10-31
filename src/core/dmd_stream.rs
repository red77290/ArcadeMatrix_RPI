//! Shared DMD Binary Stream Codec (`DMDZ` v1/v2 & `DMDS`) and Zero-Allocation Stream Player.
//!
//! Centralizes HTTP stream URL construction, unpaced `dmdz2` / `DMDS` fetching, lossless `DMDZ`
//! (`v1` & `v2`: `FRAME_DELTA`, `FRAME_SCROLL`, `FRAME_KEY`, `FRAME_SCROLL_2D`, `FRAME_RAW`,
//! `FRAME_PAL8` + `OP_LUMA`/`OP_TLUMA`/`OP_LZ`/`OP_TDIFF`) decoding, and monotonic wall-clock
//! frame pacing across all 4 Showcase rotation engines (`MediaShowcaseRunner`) and
//! `FrontendSync` (`InGameShowcaseEngine`).

use crate::core::matrix::IDrawingSurface;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DMDS_MAGIC: &[u8; 4] = b"DMDS";
pub const DMDZ_MAGIC: &[u8; 4] = b"DMDZ";
pub const DMD_STREAM_HEADER_SIZE: usize = 16;
pub const MAX_PRELOADED_FRAMES: usize = 7_600;

const MIN_DMDZ_VERSION: u8 = 1;
const MAX_DMDZ_VERSION: u8 = 2;

pub const FRAME_DELTA: u8 = 0x00;
pub const FRAME_SCROLL: u8 = 0x01;
pub const FRAME_KEY: u8 = 0x02;
pub const FRAME_SCROLL_2D: u8 = 0x03;
pub const FRAME_RAW: u8 = 0x04;
pub const FRAME_PAL8: u8 = 0x05;

const TDIFF_VECS: [[i32; 3]; 8] = [
    [-1, -2, -1],
    [-1, -1, -1],
    [0, -1, 0],
    [0, 1, 0],
    [1, 1, 1],
    [1, 2, 1],
    [0, -2, 0],
    [0, 2, 0],
];

/// Parsed 16-byte `<4sBBHHHHH` container header for `DMDZ` (`v1`/`v2`) and `DMDS` streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmdStreamHeader {
    pub is_dmdz: bool,
    pub version: u8,
    pub fps: u8,
    pub width: u16,
    pub height: u16,
    pub total_frames: u16,
    pub frame_dur_ms: u16,
    pub keyframe_interval: u16,
}

impl DmdStreamHeader {
    pub fn parse(raw16: &[u8]) -> Option<Self> {
        if raw16.len() < DMD_STREAM_HEADER_SIZE {
            return None;
        }
        let is_dmdz = &raw16[0..4] == DMDZ_MAGIC;
        let is_dmds = !is_dmdz && &raw16[0..4] == DMDS_MAGIC;
        if !is_dmdz && !is_dmds {
            return None;
        }
        let version = raw16[4];
        if is_dmdz && !(MIN_DMDZ_VERSION..=MAX_DMDZ_VERSION).contains(&version) {
            return None;
        }
        let fps = if raw16[5] > 0 { raw16[5] } else { 15 };
        let width = u16::from_le_bytes([raw16[6], raw16[7]]);
        let height = u16::from_le_bytes([raw16[8], raw16[9]]);
        let total_frames = u16::from_le_bytes([raw16[10], raw16[11]]);
        let frame_dur_ms = u16::from_le_bytes([raw16[12], raw16[13]]);
        let keyframe_interval = u16::from_le_bytes([raw16[14], raw16[15]]);
        if width == 0 || height == 0 || total_frames == 0 {
            return None;
        }
        Some(Self {
            is_dmdz,
            version,
            fps,
            width,
            height,
            total_frames,
            frame_dur_ms,
            keyframe_interval,
        })
    }

    #[inline]
    pub fn effective_frame_delay(&self) -> Duration {
        let delay_ms = if self.frame_dur_ms > 0 {
            (self.frame_dur_ms as u64).clamp(16, 500)
        } else {
            (1000 / (self.fps.max(1) as u64)).clamp(16, 500)
        };
        Duration::from_millis(delay_ms)
    }
}

#[inline]
fn decode_luma_delta_pixel(base: u16, u_dg_hi: u8, b2: u8) -> u16 {
    let u_dg = ((u_dg_hi & 0x03) << 2) | (b2 >> 6);
    let dg = (u_dg as i32) - 8;
    let dg_rb = ((u_dg >> 1) as i32) - 4;
    let dr = dg_rb + (((b2 >> 3) & 0x07) as i32) - 4;
    let db = dg_rb + ((b2 & 0x07) as i32) - 4;
    let r = ((((base >> 11) & 0x1F) as i32 + dr) & 0x1F) as u16;
    let g = ((((base >> 5) & 0x3F) as i32 + dg) & 0x3F) as u16;
    let bl = (((base & 0x1F) as i32 + db) & 0x1F) as u16;
    (r << 11) | (g << 5) | bl
}

fn apply_vertical_scroll_in_place(
    frame565: &mut [u16],
    width: usize,
    height: usize,
    dy: i32,
) -> bool {
    let total = width.saturating_mul(height);
    if frame565.len() < total {
        return false;
    }
    if dy > 0 {
        let s = dy as usize;
        if s >= height {
            return false;
        }
        frame565.copy_within((s * width)..total, 0);
    } else if dy < 0 {
        let s = (-dy) as usize;
        if s >= height {
            return false;
        }
        frame565.copy_within(0..((height - s) * width), s * width);
    }
    true
}

fn apply_scroll_2d_in_place(
    frame565: &mut [u16],
    width: usize,
    height: usize,
    dx: i32,
    dy: i32,
) -> bool {
    let abs_dx = dx.unsigned_abs() as usize;
    if abs_dx >= width || !apply_vertical_scroll_in_place(frame565, width, height, dy) {
        return false;
    }
    if dx > 0 {
        for row in 0..height {
            let r0 = row * width;
            frame565[r0..(r0 + width)].copy_within(abs_dx..width, 0);
        }
    } else if dx < 0 {
        for row in 0..height {
            let r0 = row * width;
            frame565[r0..(r0 + width)].copy_within(0..(width - abs_dx), abs_dx);
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderState {
    FrameType,
    ScrollDy,
    Scroll2DDx,
    Scroll2DDy,
    RawLo,
    RawHi,
    Op,
    LitLo,
    LitHi,
    LongSkipLo,
    LongSkipHi,
    LumaLo,
    TLumaLo,
    LzLo,
    LzHi,
    PalHdr,
    PalScrollDx,
    PalScrollDy,
    PalTableLo,
    PalTableHi,
    PalOp,
    PalLongSkipLo,
    PalLongSkipHi,
}

/// Lossless, zero-allocation, chunk-resumable `DMDZ` (`v1` & `v2`) RGB565 stream decoder.
///
/// Byte-for-byte compatible with ESP32 `core::drawing::DmdzStreamDecoder` and Python
/// `dmdz_codec.py`, supporting arbitrary chunk boundaries down to 1 byte.
#[derive(Debug, Clone)]
pub struct DmdzDecoder {
    cache: [u16; 64],
    pal: [u16; 128],
    num_pixels: usize,
    pos: usize,
    width: u16,
    height: u16,
    bytes_remaining: usize,
    last: u16,
    version: u8,
    lit_remaining: u8,
    lo_byte: u8,
    op_header: u8,
    pal_count: usize,
    pal_idx: usize,
    scroll_dx: i8,
    state: DecoderState,
    error: bool,
}

impl Default for DmdzDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DmdzDecoder {
    pub fn new() -> Self {
        Self {
            cache: [0u16; 64],
            pal: [0u16; 128],
            num_pixels: 0,
            pos: 0,
            width: 0,
            height: 0,
            bytes_remaining: 0,
            last: 0,
            version: 1,
            lit_remaining: 0,
            lo_byte: 0,
            op_header: 0,
            pal_count: 0,
            pal_idx: 0,
            scroll_dx: 0,
            state: DecoderState::FrameType,
            error: false,
        }
    }

    #[inline]
    pub fn hash_pixel(p: u16) -> usize {
        (((((p >> 11) & 0x1F) as u32 * 3)
            + (((p >> 5) & 0x3F) as u32 * 5)
            + ((p & 0x1F) as u32 * 7))
            & 63) as usize
    }

    pub fn reset_stream(&mut self, width: u16, height: u16, version: u8) {
        self.width = width;
        self.height = height;
        self.num_pixels = (width as usize).saturating_mul(height as usize);
        self.pos = 0;
        self.bytes_remaining = 0;
        self.last = 0;
        self.version = if version >= 2 { 2 } else { 1 };
        self.lit_remaining = 0;
        self.lo_byte = 0;
        self.op_header = 0;
        self.pal_count = 0;
        self.pal_idx = 0;
        self.scroll_dx = 0;
        self.state = DecoderState::FrameType;
        self.error =
            self.num_pixels == 0 || !(MIN_DMDZ_VERSION..=MAX_DMDZ_VERSION).contains(&version);
        self.cache.fill(0);
        self.pal.fill(0);
    }

    pub fn begin_frame(&mut self, payload_len: u16) -> bool {
        if self.num_pixels == 0 || payload_len == 0 {
            self.error = true;
            return false;
        }
        self.cache.fill(0);
        self.pos = 0;
        self.bytes_remaining = payload_len as usize;
        self.last = 0;
        self.lit_remaining = 0;
        self.lo_byte = 0;
        self.op_header = 0;
        self.pal_count = 0;
        self.pal_idx = 0;
        self.scroll_dx = 0;
        self.state = DecoderState::FrameType;
        self.error = false;
        true
    }

    pub fn feed(&mut self, src: &[u8], frame565: &mut [u16]) -> bool {
        if self.error || frame565.len() < self.num_pixels || src.len() > self.bytes_remaining {
            self.error = true;
            return false;
        }

        let w = self.width as usize;
        let h = self.height as usize;

        for &b in src {
            self.bytes_remaining -= 1;
            match self.state {
                DecoderState::FrameType => {
                    if b == FRAME_DELTA || b == FRAME_KEY {
                        self.state = DecoderState::Op;
                    } else if b == FRAME_SCROLL {
                        self.state = DecoderState::ScrollDy;
                    } else if self.version >= 2 && b == FRAME_SCROLL_2D {
                        self.state = DecoderState::Scroll2DDx;
                    } else if self.version >= 2 && b == FRAME_RAW {
                        if self.bytes_remaining != self.num_pixels * 2 {
                            self.error = true;
                            return false;
                        }
                        self.state = DecoderState::RawLo;
                    } else if self.version >= 2 && b == FRAME_PAL8 {
                        self.state = DecoderState::PalHdr;
                    } else {
                        self.error = true;
                        return false;
                    }
                }
                DecoderState::ScrollDy => {
                    let dy = (b as i8) as i32;
                    if !apply_vertical_scroll_in_place(frame565, w, h, dy) {
                        self.error = true;
                        return false;
                    }
                    self.state = DecoderState::Op;
                }
                DecoderState::Scroll2DDx => {
                    self.scroll_dx = b as i8;
                    self.state = DecoderState::Scroll2DDy;
                }
                DecoderState::Scroll2DDy => {
                    let dx = self.scroll_dx as i32;
                    let dy = (b as i8) as i32;
                    if !apply_scroll_2d_in_place(frame565, w, h, dx, dy) {
                        self.error = true;
                        return false;
                    }
                    self.state = DecoderState::Op;
                }
                DecoderState::RawLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::RawHi;
                }
                DecoderState::RawHi => {
                    frame565[self.pos] = (self.lo_byte as u16) | ((b as u16) << 8);
                    self.pos += 1;
                    self.state = if self.pos < self.num_pixels {
                        DecoderState::RawLo
                    } else {
                        DecoderState::Op
                    };
                }
                DecoderState::Op => {
                    if b < 0x40 {
                        let count = (b as usize) + 1;
                        if self.pos + count > self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.pos += count;
                        self.last = frame565[self.pos - 1];
                        self.cache[Self::hash_pixel(self.last)] = self.last;
                    } else if b < 0x80 {
                        let count = ((b & 0x3F) as usize) + 1;
                        if self.pos + count > self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        frame565[self.pos..(self.pos + count)].fill(self.last);
                        self.pos += count;
                    } else if b < 0xC0 {
                        if self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.last = self.cache[(b & 0x3F) as usize];
                        frame565[self.pos] = self.last;
                        self.pos += 1;
                    } else if b <= 0xDA {
                        if self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        let x = (b & 0x1F) as i32;
                        let dr = (x / 9) - 1;
                        let dg = ((x / 3) % 3) - 1;
                        let db = (x % 3) - 1;
                        let r = ((((self.last >> 11) & 0x1F) as i32 + dr) & 0x1F) as u16;
                        let g = ((((self.last >> 5) & 0x3F) as i32 + dg) & 0x3F) as u16;
                        let bl = (((self.last & 0x1F) as i32 + db) & 0x1F) as u16;
                        let p = (r << 11) | (g << 5) | bl;
                        frame565[self.pos] = p;
                        self.pos += 1;
                        self.cache[Self::hash_pixel(p)] = p;
                        self.last = p;
                    } else if (0xDC..=0xDF).contains(&b) {
                        if self.version < 2 || self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.op_header = b - 0xDC;
                        self.state = DecoderState::LumaLo;
                    } else if (0xE0..0xF0).contains(&b) {
                        let count = (b & 0x0F) + 1;
                        if self.pos + (count as usize) > self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.lit_remaining = count;
                        self.state = DecoderState::LitLo;
                    } else if b == 0xF0 {
                        self.state = DecoderState::LongSkipLo;
                    } else if (0xF1..=0xF4).contains(&b) {
                        if self.version < 2 || self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.op_header = b - 0xF1;
                        self.state = DecoderState::TLumaLo;
                    } else if b == 0xF6 || b == 0xF7 {
                        if self.version < 2 {
                            self.error = true;
                            return false;
                        }
                        self.op_header = b & 0x01;
                        self.state = DecoderState::LzLo;
                    } else if b >= 0xF8 {
                        if self.version < 2 || self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        let vec = TDIFF_VECS[(b & 0x07) as usize];
                        let ref_p = frame565[self.pos];
                        let r = ((((ref_p >> 11) & 0x1F) as i32 + vec[0]) & 0x1F) as u16;
                        let g = ((((ref_p >> 5) & 0x3F) as i32 + vec[1]) & 0x3F) as u16;
                        let bl = (((ref_p & 0x1F) as i32 + vec[2]) & 0x1F) as u16;
                        let p = (r << 11) | (g << 5) | bl;
                        frame565[self.pos] = p;
                        self.pos += 1;
                        self.cache[Self::hash_pixel(p)] = p;
                        self.last = p;
                    } else {
                        self.error = true;
                        return false;
                    }
                }
                DecoderState::LitLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::LitHi;
                }
                DecoderState::LitHi => {
                    let p = (self.lo_byte as u16) | ((b as u16) << 8);
                    frame565[self.pos] = p;
                    self.pos += 1;
                    self.cache[Self::hash_pixel(p)] = p;
                    self.last = p;
                    self.lit_remaining -= 1;
                    self.state = if self.lit_remaining > 0 {
                        DecoderState::LitLo
                    } else {
                        DecoderState::Op
                    };
                }
                DecoderState::LongSkipLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::LongSkipHi;
                }
                DecoderState::LongSkipHi => {
                    let count = ((self.lo_byte as usize) | ((b as usize) << 8)) as usize;
                    if count == 0 || self.pos + count > self.num_pixels {
                        self.error = true;
                        return false;
                    }
                    self.pos += count;
                    self.last = frame565[self.pos - 1];
                    self.cache[Self::hash_pixel(self.last)] = self.last;
                    self.state = DecoderState::Op;
                }
                DecoderState::LumaLo => {
                    let p = decode_luma_delta_pixel(self.last, self.op_header, b);
                    frame565[self.pos] = p;
                    self.pos += 1;
                    self.cache[Self::hash_pixel(p)] = p;
                    self.last = p;
                    self.state = DecoderState::Op;
                }
                DecoderState::TLumaLo => {
                    let p = decode_luma_delta_pixel(frame565[self.pos], self.op_header, b);
                    frame565[self.pos] = p;
                    self.pos += 1;
                    self.cache[Self::hash_pixel(p)] = p;
                    self.last = p;
                    self.state = DecoderState::Op;
                }
                DecoderState::LzLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::LzHi;
                }
                DecoderState::LzHi => {
                    let packed17 =
                        ((self.op_header as u32) << 16) | ((b as u32) << 8) | (self.lo_byte as u32);
                    let count = (((packed17 >> 11) & 0x3F) as usize) + 3;
                    let dist = ((packed17 & 0x7FF) as usize) + 1;
                    if dist > self.pos || self.pos + count > self.num_pixels {
                        self.error = true;
                        return false;
                    }
                    let src_idx = self.pos - dist;
                    for k in 0..count {
                        frame565[self.pos] = frame565[src_idx + k];
                        self.pos += 1;
                    }
                    self.last = frame565[self.pos - 1];
                    self.cache[Self::hash_pixel(self.last)] = self.last;
                    self.state = DecoderState::Op;
                }
                DecoderState::PalHdr => {
                    let has_scroll_2d = (b & 0x80) != 0;
                    self.pal_count = ((b & 0x7F) as usize) + 1;
                    self.pal_idx = 0;
                    self.state = if has_scroll_2d {
                        DecoderState::PalScrollDx
                    } else {
                        DecoderState::PalTableLo
                    };
                }
                DecoderState::PalScrollDx => {
                    self.scroll_dx = b as i8;
                    self.state = DecoderState::PalScrollDy;
                }
                DecoderState::PalScrollDy => {
                    let dx = self.scroll_dx as i32;
                    let dy = (b as i8) as i32;
                    if !apply_scroll_2d_in_place(frame565, w, h, dx, dy) {
                        self.error = true;
                        return false;
                    }
                    self.state = DecoderState::PalTableLo;
                }
                DecoderState::PalTableLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::PalTableHi;
                }
                DecoderState::PalTableHi => {
                    self.pal[self.pal_idx] = (self.lo_byte as u16) | ((b as u16) << 8);
                    self.pal_idx += 1;
                    if self.pal_idx < self.pal_count {
                        self.state = DecoderState::PalTableLo;
                    } else {
                        self.last = self.pal[0];
                        self.state = DecoderState::PalOp;
                    }
                }
                DecoderState::PalOp => {
                    if b < 0x80 {
                        let idx = b as usize;
                        if idx >= self.pal_count || self.pos >= self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        let p = self.pal[idx];
                        frame565[self.pos] = p;
                        self.pos += 1;
                        self.last = p;
                    } else if b <= 0xBF {
                        let count = ((b & 0x3F) as usize) + 1;
                        if self.pos + count > self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        self.pos += count;
                        self.last = frame565[self.pos - 1];
                    } else if b <= 0xFE {
                        let count = ((b - 0xC0) as usize) + 2;
                        if self.pos + count > self.num_pixels {
                            self.error = true;
                            return false;
                        }
                        frame565[self.pos..(self.pos + count)].fill(self.last);
                        self.pos += count;
                    } else {
                        self.state = DecoderState::PalLongSkipLo;
                    }
                }
                DecoderState::PalLongSkipLo => {
                    self.lo_byte = b;
                    self.state = DecoderState::PalLongSkipHi;
                }
                DecoderState::PalLongSkipHi => {
                    let count = (self.lo_byte as usize) | ((b as usize) << 8);
                    if count == 0 || self.pos + count > self.num_pixels {
                        self.error = true;
                        return false;
                    }
                    self.pos += count;
                    self.last = frame565[self.pos - 1];
                    self.state = DecoderState::PalOp;
                }
            }
        }
        true
    }

    #[inline]
    pub fn finish_frame(&self) -> bool {
        !self.error
            && self.bytes_remaining == 0
            && (self.state == DecoderState::Op || self.state == DecoderState::PalOp)
            && self.pos == self.num_pixels
    }

    pub fn decode_frame(&mut self, payload: &[u8], frame565: &mut [u16]) -> bool {
        let Ok(len_u16) = u16::try_from(payload.len()) else {
            self.error = true;
            return false;
        };
        self.begin_frame(len_u16) && self.feed(payload, frame565) && self.finish_frame()
    }
}

/// Internal storage backing a preloaded DMD binary stream (`DMDZ` compressed or legacy `DMDS` raw).
#[derive(Debug, Clone)]
pub enum DmdStreamStorage {
    /// Uncompressed little-endian RGB565 frames (`frame_count * width * height * 2` bytes).
    RawRgb565Le(Vec<u8>),
    /// Lossless `DMDZ` (`v1` or `v2`) compressed stream (`[u16 payload_len][ops...]`) with
    /// pre-indexed byte offsets (`frame_offsets.len() == frame_count + 1`) into `payload`.
    CompressedDmdz {
        version: u8,
        keyframe_interval: u16,
        payload: Vec<u8>,
        frame_offsets: Vec<u32>,
    },
}

/// Single-allocation preloaded Showcase stream (`DMDZ` v1/v2 or legacy `DMDS` RGB565LE).
///
/// Storing compressed `DMDZ` v2 directly in RAM shrinks 60s `256x64` sequences from `57.6 MB`
/// down to `~2–4 MB` (~12x–20x smaller) while preserving 100% lossless True-RGB565 output and
/// immediate OS page reclamation on `Drop`.
#[derive(Debug, Clone)]
pub struct PreloadedDmdStream {
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub frame_delay: Duration,
    storage: DmdStreamStorage,
}

impl PreloadedDmdStream {
    pub fn from_raw_rgb565_le(
        width: u32,
        height: u32,
        frame_count: usize,
        frame_delay: Duration,
        raw_rgb565_le: Vec<u8>,
    ) -> Self {
        Self {
            width,
            height,
            frame_count,
            frame_delay,
            storage: DmdStreamStorage::RawRgb565Le(raw_rgb565_le),
        }
    }

    #[inline]
    pub fn is_compressed_dmdz(&self) -> bool {
        matches!(self.storage, DmdStreamStorage::CompressedDmdz { .. })
    }

    #[inline]
    pub fn stream_version(&self) -> u8 {
        match &self.storage {
            DmdStreamStorage::RawRgb565Le(_) => 1,
            DmdStreamStorage::CompressedDmdz { version, .. } => *version,
        }
    }

    #[inline]
    pub fn frame_bytes(&self) -> usize {
        (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(2)
    }

    #[inline]
    pub fn payload_byte_len(&self) -> usize {
        match &self.storage {
            DmdStreamStorage::RawRgb565Le(raw) => raw.len(),
            DmdStreamStorage::CompressedDmdz { payload, .. } => payload.len(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.frame_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.frame_count == 0 || self.payload_byte_len() == 0
    }

    #[inline]
    fn compressed_frame_slice(&self, frame_idx: usize) -> Option<&[u8]> {
        match &self.storage {
            DmdStreamStorage::CompressedDmdz {
                payload,
                frame_offsets,
                ..
            } => {
                if frame_idx + 1 >= frame_offsets.len() {
                    return None;
                }
                let start = (frame_offsets[frame_idx] as usize).checked_add(2)?;
                let end = frame_offsets[frame_idx + 1] as usize;
                if start <= end && end <= payload.len() {
                    Some(&payload[start..end])
                } else {
                    None
                }
            }
            DmdStreamStorage::RawRgb565Le(_) => None,
        }
    }

    #[inline]
    fn is_self_contained_keyframe(&self, frame_idx: usize) -> bool {
        let version = self.stream_version();
        let Some(slice) = self.compressed_frame_slice(frame_idx) else {
            return false;
        };
        let Some(&first) = slice.first() else {
            return false;
        };
        first == FRAME_KEY || (version >= 2 && first == FRAME_RAW)
    }

    /// Stateless single-frame draw helper (uses `O(1)` slice blit for `DMDS` or keyframe-assisted
    /// decode for `DMDZ`). Continuous playback should prefer [`ShowcaseStreamPlayer`] so sequential
    /// delta frames decode in `O(1)` with zero heap allocations.
    pub fn draw_frame_to_surface(&self, frame_idx: usize, matrix: &mut dyn IDrawingSurface) {
        if self.is_empty() || self.width == 0 || self.height == 0 {
            return;
        }
        let idx = frame_idx.min(self.frame_count - 1);
        match &self.storage {
            DmdStreamStorage::RawRgb565Le(raw) => {
                blit_raw_rgb565_le_frame(raw, self.width, self.height, idx, matrix);
            }
            DmdStreamStorage::CompressedDmdz { version, .. } => {
                let mut decoder = DmdzDecoder::new();
                decoder.reset_stream(self.width as u16, self.height as u16, *version);
                let num_pixels = (self.width as usize).saturating_mul(self.height as usize);
                let mut buf = vec![0u16; num_pixels];
                let mut start_k = 0usize;
                for k in (1..=idx).rev() {
                    if self.is_self_contained_keyframe(k) {
                        start_k = k;
                        break;
                    }
                }
                for k in start_k..=idx {
                    let Some(slice) = self.compressed_frame_slice(k) else {
                        return;
                    };
                    if !decoder.decode_frame(slice, &mut buf) {
                        return;
                    }
                }
                blit_u16_rgb565_frame(&buf, self.width, self.height, matrix);
            }
        }
    }
}

#[inline]
fn blit_raw_rgb565_le_frame(
    raw: &[u8],
    width: u32,
    height: u32,
    frame_idx: usize,
    matrix: &mut dyn IDrawingSurface,
) {
    let fb = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(2);
    let start = frame_idx.saturating_mul(fb);
    if fb == 0 || start + fb > raw.len() {
        return;
    }
    let slice = &raw[start..(start + fb)];
    let fw = (width as i32).min(matrix.width() as i32);
    let fh = (height as i32).min(matrix.height() as i32);
    let stride = (width as usize) * 2;
    for py in 0..fh {
        let row_off = (py as usize) * stride;
        for px in 0..fw {
            let off = row_off + (px as usize) * 2;
            let p = (slice[off] as u16) | ((slice[off + 1] as u16) << 8);
            let r5 = ((p >> 11) & 0x1F) as u8;
            let g6 = ((p >> 5) & 0x3F) as u8;
            let b5 = (p & 0x1F) as u8;
            matrix.set_pixel(
                px,
                py,
                (r5 << 3) | (r5 >> 2),
                (g6 << 2) | (g6 >> 4),
                (b5 << 3) | (b5 >> 2),
            );
        }
    }
}

#[inline]
fn blit_u16_rgb565_frame(buf: &[u16], width: u32, height: u32, matrix: &mut dyn IDrawingSurface) {
    let total = (width as usize).saturating_mul(height as usize);
    if total == 0 || buf.len() < total {
        return;
    }
    let fw = (width as i32).min(matrix.width() as i32);
    let fh = (height as i32).min(matrix.height() as i32);
    let stride = width as usize;
    for py in 0..fh {
        let row_off = (py as usize) * stride;
        for px in 0..fw {
            let p = buf[row_off + (px as usize)];
            let r5 = ((p >> 11) & 0x1F) as u8;
            let g6 = ((p >> 5) & 0x3F) as u8;
            let b5 = (p & 0x1F) as u8;
            matrix.set_pixel(
                px,
                py,
                (r5 << 3) | (r5 >> 2),
                (g6 << 2) | (g6 >> 4),
                (b5 << 3) | (b5 >> 2),
            );
        }
    }
}

/// Parse a binary Showcase stream (`DMDZ` v1/v2 or legacy `DMDS` RGB565LE) into a single
/// contiguous `Arc<PreloadedDmdStream>`.
pub fn parse_dmd_stream_compact(bytes: &[u8]) -> Option<Arc<PreloadedDmdStream>> {
    let hdr = DmdStreamHeader::parse(bytes)?;
    let width = hdr.width as u32;
    let height = hdr.height as u32;
    let max_frames = (hdr.total_frames as usize).min(MAX_PRELOADED_FRAMES);
    if max_frames == 0 {
        return None;
    }

    if hdr.is_dmdz {
        let body = &bytes[DMD_STREAM_HEADER_SIZE..];
        let mut offsets = Vec::with_capacity(max_frames + 1);
        let mut cursor = 0usize;
        for _ in 0..max_frames {
            if cursor + 2 > body.len() {
                break;
            }
            let pay_len = u16::from_le_bytes([body[cursor], body[cursor + 1]]) as usize;
            if pay_len == 0 || cursor + 2 + pay_len > body.len() {
                break;
            }
            let Ok(off_u32) = u32::try_from(cursor) else {
                break;
            };
            offsets.push(off_u32);
            cursor += 2 + pay_len;
        }
        let available_frames = offsets.len();
        let Ok(end_u32) = u32::try_from(cursor) else {
            return None;
        };
        if available_frames == 0 || cursor == 0 {
            return None;
        }
        offsets.push(end_u32);
        let payload = body[..cursor].to_vec();
        Some(Arc::new(PreloadedDmdStream {
            width,
            height,
            frame_count: available_frames,
            frame_delay: hdr.effective_frame_delay(),
            storage: DmdStreamStorage::CompressedDmdz {
                version: hdr.version,
                keyframe_interval: hdr.keyframe_interval,
                payload,
                frame_offsets: offsets,
            },
        }))
    } else {
        let frame_bytes = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(2)?;
        if frame_bytes == 0 {
            return None;
        }
        let payload_len = bytes.len().saturating_sub(DMD_STREAM_HEADER_SIZE);
        let available_frames = (payload_len / frame_bytes).min(max_frames);
        if available_frames == 0 {
            return None;
        }
        let valid_byte_len = available_frames * frame_bytes;
        let raw_rgb565_le =
            bytes[DMD_STREAM_HEADER_SIZE..(DMD_STREAM_HEADER_SIZE + valid_byte_len)].to_vec();
        Some(Arc::new(PreloadedDmdStream {
            width,
            height,
            frame_count: available_frames,
            frame_delay: hdr.effective_frame_delay(),
            storage: DmdStreamStorage::RawRgb565Le(raw_rgb565_le),
        }))
    }
}

/// Select the Showcase server stream profile matching the target panel geometry (`psram` vs `psram_hd`).
#[inline]
pub fn select_stream_profile(width: u32, height: u32) -> &'static str {
    if (width as u64).saturating_mul(height as u64) > 8_192 {
        "psram_hd"
    } else {
        "psram"
    }
}

/// Build the canonical unpaced `dmdz2` HTTP stream URL for `dmd-media-showcase`, normalizing any
/// pre-existing query parameters while preserving optional `api_key` parameters.
pub fn build_showcase_stream_url(
    base_url: &str,
    raw_path: &str,
    client_id: Option<&str>,
    purge: bool,
    width: u32,
    height: u32,
) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let trimmed_path = raw_path.trim();
    let (path_only, existing_query) = match trimmed_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (trimmed_path, ""),
    };
    let clean_path_owned;
    let clean_path = if path_only.starts_with('/') || path_only.is_empty() {
        path_only
    } else {
        clean_path_owned = format!("/{}", path_only);
        clean_path_owned.as_str()
    };

    let mut preserved_params: Vec<&str> = Vec::new();
    for kv in existing_query.split('&') {
        let kv = kv.trim();
        if kv.is_empty() {
            continue;
        }
        let key = kv.split_once('=').map_or(kv, |(k, _)| k);
        if matches!(
            key.to_ascii_lowercase().as_str(),
            "purge" | "paced" | "codec" | "profile" | "client_id" | "drop_late"
        ) {
            continue;
        }
        preserved_params.push(kv);
    }

    let profile = select_stream_profile(width, height);
    let mut url = format!(
        "{}{}?purge={}&paced=0&codec=dmdz2&profile={}",
        base,
        clean_path,
        if purge { "true" } else { "false" },
        profile
    );
    if let Some(cid) = client_id.map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str("&client_id=");
        url.push_str(cid);
    }
    for extra in preserved_params {
        url.push('&');
        url.push_str(extra);
    }
    url
}

/// Download and parse a Showcase stream (`DMDZ` v2 with automatic `DMDS` fallback) using the
/// process-wide connection-pooled `stream_http_client()`.
pub fn fetch_showcase_stream(
    base_url: &str,
    raw_path: &str,
    client_id: Option<&str>,
    purge: bool,
    width: u32,
    height: u32,
    timeout: Option<Duration>,
) -> Option<Arc<PreloadedDmdStream>> {
    if base_url.trim().is_empty() || raw_path.trim().is_empty() {
        return None;
    }
    let url = build_showcase_stream_url(base_url, raw_path, client_id, purge, width, height);
    let client = crate::core::memory::stream_http_client();
    let mut req = client.get(&url);
    if let Some(t) = timeout {
        req = req.timeout(t);
    }
    let resp = req.send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().ok()?;
    parse_dmd_stream_compact(&bytes)
}

/// Shared stateful zero-reallocation player for `PreloadedDmdStream` (`DMDZ` v1/v2 & `DMDS`).
///
/// Reused 1:1 by both `MediaShowcaseRunner` (rotation Showcase engines) and
/// `InGameShowcaseEngine` (`FrontendSync` Late Preemption):
/// - Drift-free monotonic wall-clock frame indexing (with pause/resume support).
/// - Single reusable `Vec<u16>` decode buffer (`width * height` pixels = `8..32 KB`) with `0`
///   per-frame heap allocations.
/// - Clean `Loop` wrap-around and keyframe-assisted fast forward.
/// - Automatic border clearing when stream dimensions differ from the active `IDrawingSurface`.
#[derive(Debug, Clone)]
pub struct ShowcaseStreamPlayer {
    decoder: DmdzDecoder,
    frame_rgb565: Vec<u16>,
    bound_stream_ptr: usize,
    decoded_frame_idx: Option<usize>,
    decoded_loop_epoch: u64,
    target_frame_idx: usize,
    loop_epoch: u64,
    started_at: Option<Instant>,
    accumulated_elapsed: Duration,
    paused: bool,
    finished: bool,
    last_drawn_key: Option<(usize, u64, usize)>,
}

impl Default for ShowcaseStreamPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ShowcaseStreamPlayer {
    pub fn new() -> Self {
        Self {
            decoder: DmdzDecoder::new(),
            frame_rgb565: Vec::new(),
            bound_stream_ptr: 0,
            decoded_frame_idx: None,
            decoded_loop_epoch: 0,
            target_frame_idx: 0,
            loop_epoch: 0,
            started_at: None,
            accumulated_elapsed: Duration::ZERO,
            paused: false,
            finished: false,
            last_drawn_key: None,
        }
    }

    pub fn start(&mut self, now: Instant) {
        self.bound_stream_ptr = 0;
        self.decoded_frame_idx = None;
        self.decoded_loop_epoch = 0;
        self.target_frame_idx = 0;
        self.loop_epoch = 0;
        self.started_at = Some(now);
        self.accumulated_elapsed = Duration::ZERO;
        self.paused = false;
        self.finished = false;
        self.last_drawn_key = None;
    }

    pub fn reset(&mut self) {
        self.bound_stream_ptr = 0;
        self.decoded_frame_idx = None;
        self.decoded_loop_epoch = 0;
        self.target_frame_idx = 0;
        self.loop_epoch = 0;
        self.started_at = None;
        self.accumulated_elapsed = Duration::ZERO;
        self.paused = false;
        self.finished = false;
        self.last_drawn_key = None;
        self.frame_rgb565.clear();
        self.frame_rgb565.shrink_to_fit();
    }

    pub fn pause(&mut self, now: Instant) {
        if !self.paused {
            if let Some(st) = self.started_at.take() {
                self.accumulated_elapsed = self
                    .accumulated_elapsed
                    .saturating_add(now.saturating_duration_since(st));
            }
            self.paused = true;
        }
    }

    pub fn resume(&mut self, now: Instant) {
        self.paused = false;
        self.started_at = Some(now);
        self.last_drawn_key = None;
    }

    #[inline]
    pub fn invalidate_drawn_frame(&mut self) {
        self.last_drawn_key = None;
    }

    #[inline]
    pub fn is_started(&self) -> bool {
        self.started_at.is_some() || self.paused
    }

    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn current_frame_index(&self) -> usize {
        self.target_frame_idx
    }

    #[inline]
    pub fn has_new_frame(&self, stream: &Arc<PreloadedDmdStream>) -> bool {
        if self.finished || self.paused || stream.is_empty() {
            return false;
        }
        let key = (
            Arc::as_ptr(stream) as usize,
            self.loop_epoch,
            self.target_frame_idx,
        );
        self.last_drawn_key != Some(key)
    }

    /// Advance playback using the player's internal pause-aware monotonic clock.
    pub fn advance(
        &mut self,
        stream: &Arc<PreloadedDmdStream>,
        now: Instant,
        loop_playback: bool,
        max_duration: Option<Duration>,
    ) -> bool {
        if self.paused {
            return self.finished;
        }
        let st = *self.started_at.get_or_insert(now);
        let elapsed = self
            .accumulated_elapsed
            .saturating_add(now.saturating_duration_since(st));
        self.advance_at_elapsed(stream, elapsed, loop_playback, max_duration)
    }

    /// Advance playback at an explicit monotonic `elapsed` offset from stream start.
    pub fn advance_at_elapsed(
        &mut self,
        stream: &Arc<PreloadedDmdStream>,
        elapsed: Duration,
        loop_playback: bool,
        max_duration: Option<Duration>,
    ) -> bool {
        if stream.is_empty() {
            self.finished = true;
            return true;
        }
        if !loop_playback {
            if let Some(max_dur) = max_duration {
                if elapsed >= max_dur {
                    self.finished = true;
                    return true;
                }
            }
        }
        let frame_us = stream.frame_delay.as_micros().max(1_000);
        let raw_step = (elapsed.as_micros() / frame_us) as usize;
        if loop_playback {
            self.loop_epoch = (raw_step / stream.frame_count) as u64;
            self.target_frame_idx = raw_step % stream.frame_count;
            self.finished = false;
            false
        } else if raw_step >= stream.frame_count {
            self.target_frame_idx = stream.frame_count - 1;
            self.finished = true;
            true
        } else {
            self.target_frame_idx = raw_step;
            self.finished = false;
            false
        }
    }

    fn ensure_dmdz_decoded_to(&mut self, stream: &Arc<PreloadedDmdStream>, target: usize) -> bool {
        let num_pixels = (stream.width as usize).saturating_mul(stream.height as usize);
        if num_pixels == 0 {
            return false;
        }
        let stream_ptr = Arc::as_ptr(stream) as usize;
        if self.frame_rgb565.len() != num_pixels {
            self.frame_rgb565.resize(num_pixels, 0);
            self.decoded_frame_idx = None;
        }
        if self.bound_stream_ptr != stream_ptr {
            self.bound_stream_ptr = stream_ptr;
            self.decoded_frame_idx = None;
            self.decoded_loop_epoch = self.loop_epoch;
            self.decoder.reset_stream(
                stream.width as u16,
                stream.height as u16,
                stream.stream_version(),
            );
        }

        let need_restart = match self.decoded_frame_idx {
            None => true,
            Some(cur) => self.decoded_loop_epoch != self.loop_epoch || target < cur,
        };

        let start_k = if need_restart {
            self.decoded_loop_epoch = self.loop_epoch;
            self.decoder.reset_stream(
                stream.width as u16,
                stream.height as u16,
                stream.stream_version(),
            );
            let mut key_k = 0usize;
            for k in (1..=target).rev() {
                if stream.is_self_contained_keyframe(k) {
                    key_k = k;
                    break;
                }
            }
            if key_k == 0 {
                self.frame_rgb565.fill(0);
            }
            key_k
        } else {
            let cur = self.decoded_frame_idx.unwrap();
            if cur == target {
                return true;
            }
            let mut key_k = cur + 1;
            for k in ((cur + 1)..=target).rev() {
                if stream.is_self_contained_keyframe(k) {
                    key_k = k;
                    break;
                }
            }
            key_k
        };

        for k in start_k..=target {
            let Some(slice) = stream.compressed_frame_slice(k) else {
                return false;
            };
            if !self.decoder.decode_frame(slice, &mut self.frame_rgb565) {
                self.decoded_frame_idx = None;
                return false;
            }
        }
        self.decoded_frame_idx = Some(target);
        true
    }

    /// Decode (if needed) and blit the current frame onto `surface`, clearing borders automatically
    /// whenever the stream dimensions differ from the surface dimensions.
    pub fn render_to_surface(
        &mut self,
        stream: &Arc<PreloadedDmdStream>,
        surface: &mut dyn IDrawingSurface,
    ) {
        if stream.is_empty() || stream.width == 0 || stream.height == 0 {
            return;
        }
        let idx = self.target_frame_idx.min(stream.frame_count - 1);
        if stream.width != surface.width() || stream.height != surface.height() {
            surface.clear();
        }
        match &stream.storage {
            DmdStreamStorage::RawRgb565Le(raw) => {
                blit_raw_rgb565_le_frame(raw, stream.width, stream.height, idx, surface);
            }
            DmdStreamStorage::CompressedDmdz { .. } => {
                if !self.ensure_dmdz_decoded_to(stream, idx) {
                    return;
                }
                blit_u16_rgb565_frame(&self.frame_rgb565, stream.width, stream.height, surface);
            }
        }
        self.last_drawn_key = Some((Arc::as_ptr(stream) as usize, self.loop_epoch, idx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockSurface;

    fn pack_dmdz_container(
        version: u8,
        fps: u8,
        width: u16,
        height: u16,
        frame_dur_ms: u16,
        keyframe_interval: u16,
        frames: &[Vec<u8>],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(DMDZ_MAGIC);
        out.push(version);
        out.push(fps);
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
        out.extend_from_slice(&frame_dur_ms.to_le_bytes());
        out.extend_from_slice(&keyframe_interval.to_le_bytes());
        for f in frames {
            out.extend_from_slice(&(f.len() as u16).to_le_bytes());
            out.extend_from_slice(f);
        }
        out
    }

    #[test]
    fn test_dmdz_v2_all_frame_types_and_chunk_resumable_feed() {
        // 4x2 = 8 pixels stream exercising:
        // Frame 0: FRAME_RAW (0x04)
        // Frame 1: FRAME_SCROLL_2D (0x03) + OP_LUMA + OP_TLUMA + OP_LZ + OP_TDIFF
        // Frame 2: FRAME_PAL8 (0x05) with 2D scroll + PAL_INDEX + PAL_RUN + PAL_SKIP
        let w = 4u16;
        let h = 2u16;
        let p_red: u16 = 0xF800;
        let p_green: u16 = 0x07E0;
        let p_blue: u16 = 0x001F;
        let p_mid: u16 = (16u16 << 11) | (32u16 << 5) | 16u16;

        // Frame 0: [p_mid, p_red, p_green, p_blue, p_mid, p_mid, p_mid, p_mid]
        let f0_pixels = [p_mid, p_red, p_green, p_blue, p_mid, p_mid, p_mid, p_mid];
        let mut f0 = vec![FRAME_RAW];
        for px in f0_pixels {
            f0.extend_from_slice(&px.to_le_bytes());
        }

        // Frame 1: FRAME_SCROLL_2D dx=1, dy=0 -> row0 becomes [p_red, p_green, p_blue, p_blue], row1 [p_mid; 4]
        // Then:
        //   pos 0: OP_SKIP 1 -> keeps p_red, last = p_red
        //   pos 1: OP_TDIFF idx 3 ([0, +1, 0]) on ref p_green (g=63+1 -> 0) -> 0x0000
        //   pos 2: OP_TLUMA on ref p_blue (r=0,g=0,b=31) with dg=0(u_dg=8 -> hi=2, lo_top=0), dr=0(4), db=0(4) -> keeps p_blue
        //   pos 3: OP_LUMA on last=p_blue with dg=+2(u_dg=10 -> hi=2, lo_top=2), dr=+1(4), db=-1(2):
        //          dg_rb = (10>>1)-4 = +1; dr = 1 + (3-4) = 0; db = 1 + (2-4) = -1 -> r=0, g=2, b=30
        //   pos 4..7 (4 pixels): OP_LZ count=4 (mlen-3=1 -> 1<<11), dist=4 (dist-1=3) copying pos 0..4!
        let mut f1 = vec![FRAME_SCROLL_2D, 1u8, 0u8];
        f1.push(0x00); // OP_SKIP 1 (pos -> 1)
        f1.push(0xF8 | 3); // OP_TDIFF vec[3] = [0, +1, 0] (pos -> 2)
        f1.push(0xF1 | 2); // OP_TLUMA u_dg_hi = 2
        f1.push((0 << 6) | (4 << 3) | 4); // u_dg=8 (dg=0, dg_rb=0), dr=0, db=0 -> p_blue (pos -> 3)
        f1.push(0xDC | 2); // OP_LUMA u_dg_hi = 2
        f1.push((2 << 6) | (3 << 3) | 2); // u_dg=10 (dg=+2, dg_rb=+1), dr=0, db=-1 -> (0, 2, 30) (pos -> 4)
        let packed17: u32 = (1u32 << 11) | 3u32; // count = 4, dist = 4
        f1.push(0xF6 | ((packed17 >> 16) as u8 & 0x01));
        f1.push((packed17 & 0xFF) as u8);
        f1.push(((packed17 >> 8) & 0xFF) as u8);

        let expected_p3: u16 = (2u16 << 5) | 30u16;
        let expected_f1 = [
            p_red,
            0x0000,
            p_blue,
            expected_p3,
            p_red,
            0x0000,
            p_blue,
            expected_p3,
        ];

        // Frame 2: FRAME_PAL8 with hasScroll2D (dx=-1, dy=0) + 2 palette colors [0xFFFF, 0xF81F]
        // After dx=-1 scroll on expected_f1: [p_red, p_red, 0x0000, p_blue, p_red, p_red, 0x0000, p_blue]
        //   pos 0: PAL_INDEX 1 (0xF81F), last = 0xF81F
        //   pos 1..3 (3 pixels): PAL_RUN count=3 (0xC0 + 1) -> 0xF81F
        //   pos 4..5 (2 pixels): PAL_SKIP count=2 (0x80 | 1) -> keeps [p_red, p_red]
        //   pos 6..7 (2 pixels): PAL_LONG_SKIP count=2 (0xFF, 0x02, 0x00) -> keeps [0x0000, p_blue]
        let mut f2 = vec![FRAME_PAL8, 0x80 | 1, (-1i8) as u8, 0u8];
        f2.extend_from_slice(&0xFFFFu16.to_le_bytes());
        f2.extend_from_slice(&0xF81Fu16.to_le_bytes());
        f2.push(1); // PAL_INDEX 1 -> pos 1
        f2.push(0xC1); // PAL_RUN 3 -> pos 4
        f2.push(0x81); // PAL_SKIP 2 -> pos 6
        f2.extend_from_slice(&[0xFF, 2, 0]); // PAL_LONG_SKIP 2 -> pos 8

        let expected_f2 = [0xF81F, 0xF81F, 0xF81F, 0xF81F, p_red, p_red, 0x0000, p_blue];

        // Verify 1-byte chunk-resumable decoding matches full-frame decoding
        let mut dec = DmdzDecoder::new();
        dec.reset_stream(w, h, 2);
        let mut buf = [0u16; 8];
        for (frame_bytes, expected) in [(&f0, &f0_pixels), (&f1, &expected_f1), (&f2, &expected_f2)]
        {
            assert!(dec.begin_frame(frame_bytes.len() as u16));
            for b in frame_bytes {
                assert!(dec.feed(std::slice::from_ref(b), &mut buf));
            }
            assert!(dec.finish_frame());
            assert_eq!(&buf, expected);
        }

        // Verify container parser + ShowcaseStreamPlayer playback & Loop wrap-around
        let container = pack_dmdz_container(2, 25, w, h, 40, 0, &[f0, f1, f2]);
        let stream = parse_dmd_stream_compact(&container).expect("valid DMDZ v2 stream");
        assert!(stream.is_compressed_dmdz());
        assert_eq!(stream.stream_version(), 2);
        assert_eq!(stream.frame_count, 3);
        assert_eq!(stream.frame_delay, Duration::from_millis(40));

        let mut player = ShowcaseStreamPlayer::new();
        let t0 = Instant::now();
        player.start(t0);
        let mut surface = MockSurface::new(4, 2);

        // Frame 0 -> Frame 1 -> Frame 2
        assert!(!player.advance(&stream, t0 + Duration::from_millis(85), false, None));
        assert_eq!(player.current_frame_index(), 2);
        assert!(player.has_new_frame(&stream));
        player.render_to_surface(&stream, &mut surface);
        assert!(!player.has_new_frame(&stream));
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [255, 0, 255]); // 0xF81F magenta
        assert_eq!(surface.canvas.get_pixel(0, 1).0, [255, 0, 0]); // p_red
        assert_eq!(surface.canvas.get_pixel(3, 1).0, [0, 0, 255]); // p_blue

        // Loop wrap-around at t = 125ms (step 3 % 3 == 0 -> Frame 0!)
        assert!(!player.advance(&stream, t0 + Duration::from_millis(125), true, None));
        assert_eq!(player.current_frame_index(), 0);
        assert!(player.has_new_frame(&stream));
        player.render_to_surface(&stream, &mut surface);
        assert_eq!(surface.canvas.get_pixel(1, 0).0, [255, 0, 0]); // f0[1] == p_red
    }

    #[test]
    fn test_build_showcase_stream_url_normalizes_query_and_preserves_api_key() {
        let u1 = build_showcase_stream_url(
            "http://192.168.1.50:8890/",
            "/api/v1/movies/stream/pkg_movies_abc123.rgb565.stream?purge=true&api_key=sec123",
            Some("rpi_deadbe_movies_128x32"),
            false,
            128,
            32,
        );
        assert_eq!(
            u1,
            "http://192.168.1.50:8890/api/v1/movies/stream/pkg_movies_abc123.rgb565.stream?purge=false&paced=0&codec=dmdz2&profile=psram&client_id=rpi_deadbe_movies_128x32&api_key=sec123"
        );

        let u2 = build_showcase_stream_url(
            "http://192.168.1.50:8890",
            "/api/v1/games/stream/pkg_games_999.rgb565.stream",
            None,
            true,
            256,
            64,
        );
        assert_eq!(
            u2,
            "http://192.168.1.50:8890/api/v1/games/stream/pkg_games_999.rgb565.stream?purge=true&paced=0&codec=dmdz2&profile=psram_hd"
        );
    }

    #[test]
    fn test_player_pause_resume_and_smaller_stream_clears_borders() {
        let mut raw = Vec::new();
        raw.extend_from_slice(DMDS_MAGIC);
        raw.push(1);
        raw.push(20);
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.extend_from_slice(&50u16.to_le_bytes());
        raw.extend_from_slice(&0u16.to_le_bytes());
        for _ in 0..4 {
            raw.extend_from_slice(&0x07E0u16.to_le_bytes());
        }
        for _ in 0..4 {
            raw.extend_from_slice(&0xF800u16.to_le_bytes());
        }
        let stream = parse_dmd_stream_compact(&raw).expect("valid DMDS fallback");
        assert!(!stream.is_compressed_dmdz());

        let mut surface = MockSurface::new(4, 4);
        // Pre-fill surface with non-zero white pixels to verify border clearing
        surface.fill_rect(0, 0, 4, 4, 255, 255, 255);

        let mut player = ShowcaseStreamPlayer::new();
        let t0 = Instant::now();
        player.start(t0);
        player.advance(&stream, t0 + Duration::from_millis(20), false, None);
        player.render_to_surface(&stream, &mut surface);
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [0, 255, 0]);
        assert_eq!(surface.canvas.get_pixel(3, 3).0, [0, 0, 0]);

        // Pause at +20ms, wait 500ms while paused, resume at +520ms -> at +530ms total active time is 30ms (still frame 0!)
        player.pause(t0 + Duration::from_millis(20));
        assert!(!player.has_new_frame(&stream));
        player.resume(t0 + Duration::from_millis(520));
        assert!(player.has_new_frame(&stream));
        player.advance(&stream, t0 + Duration::from_millis(530), false, None);
        assert_eq!(player.current_frame_index(), 0);
        assert!(!player.is_finished());

        // At +560ms total active time is 60ms -> frame 1
        player.advance(&stream, t0 + Duration::from_millis(560), false, None);
        assert_eq!(player.current_frame_index(), 1);
    }
}
