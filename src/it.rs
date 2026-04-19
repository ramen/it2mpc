// The public structs here represent the full IT file format; not every field
// is used by the current conversion code but they are kept for completeness.
#![allow(dead_code)]

/// Impulse Tracker (.IT) file parser.

use anyhow::{ensure, Result};

// ---------------------------------------------------------------------------
// Low-level reader helpers
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(self.pos + n <= self.data.len(), "unexpected end of file at offset {}", self.pos);
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        let b = self.read_bytes(1)?;
        Ok(b[0])
    }

    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    fn u16_le(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_le(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn flags8(&mut self) -> Result<u8> {
        self.u8()
    }

    fn flags16(&mut self) -> Result<u16> {
        self.u16_le()
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        ensure!(self.pos + n <= self.data.len(), "unexpected end of file (skip) at offset {}", self.pos);
        self.pos += n;
        Ok(())
    }

    /// Read a CP437/Latin-1 string of fixed byte length, trimming NUL bytes.
    fn string(&mut self, len: usize) -> Result<String> {
        let bytes = self.read_bytes(len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
        // Use a best-effort conversion: replace non-ASCII bytes with '?'.
        Ok(bytes[..end]
            .iter()
            .map(|&b| if b.is_ascii() { b as char } else { '?' })
            .collect())
    }

    fn magic_check(&mut self, magic: &[u8]) -> Result<()> {
        let got = self.read_bytes(magic.len())?;
        ensure!(got == magic, "bad magic: expected {:?}, got {:?}", magic, got);
        Ok(())
    }

}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Envelope {
    pub flags: u8,
    pub num_points: u8,
    pub loop_start: u8,
    pub loop_end: u8,
    pub susloop_start: u8,
    pub susloop_end: u8,
    /// Each point is (value: i8, tick_position: u16)
    pub points: Vec<(i8, u16)>,
}

impl Envelope {
    pub fn loop_enabled(&self) -> bool {
        self.flags & 0x02 != 0
    }

    pub fn sustain_enabled(&self) -> bool {
        self.flags & 0x04 != 0
    }

    pub fn envelope_enabled(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// True if this is actually a filter envelope (pitch env flag bit 7).
    pub fn is_filter(&self) -> bool {
        self.flags & 0x80 != 0
    }

    fn parse(r: &mut Reader) -> Result<Self> {
        let flags = r.flags8()?;
        let num_points = r.u8()?;
        let loop_start = r.u8()?;
        let loop_end = r.u8()?;
        let susloop_start = r.u8()?;
        let susloop_end = r.u8()?;

        let n = num_points as usize;
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let val = r.i8()?;
            let tick = r.u16_le()?;
            points.push((val, tick));
        }
        // Each point is 3 bytes; total space is 75 bytes (25×3), skip remainder.
        let used = n * 3;
        r.skip(75usize.saturating_sub(used))?;

        Ok(Self {
            flags,
            num_points,
            loop_start,
            loop_end,
            susloop_start,
            susloop_end,
            points,
        })
    }
}

// ---------------------------------------------------------------------------
// Sample
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Sample {
    pub dos_filename: String,
    pub global_vol: u8,
    pub flags: u8,
    pub default_vol: u8,
    pub name: String,
    pub length: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub c5_speed: u32,
    pub susloop_start: u32,
    pub susloop_end: u32,
    pub sample_pointer: u32,
    pub vibrato_speed: u8,
    pub vibrato_depth: u8,
    pub vibrato_sweep: u8,
    pub vibrato_wave: u8,
    pub bits16: bool,
    pub stereo: bool,
    pub loop_active: bool,
    pub sustain_loop_active: bool,
    pub pingpong: bool,
    pub sustain_pingpong: bool,
    pub compressed: bool,
    /// Sample convert flags (bit 0 = signed, bit 2 = IT2.15 double-delta compression)
    pub convert_flags: u8,
}

impl Sample {
    fn parse(data: &[u8], ptr: u32) -> Result<Self> {
        let ptr = ptr as usize;
        ensure!(ptr + 80 <= data.len(), "sample pointer out of range: {ptr}");
        let mut r = Reader::new(data);
        r.seek(ptr);
        r.magic_check(b"IMPS")?;
        let dos_filename = r.string(12)?;
        r.skip(1)?; // padding / always zero
        let global_vol = r.u8()?;
        let flags = r.u8()?;
        let default_vol = r.u8()?;
        let name = r.string(26)?;
        let convert_flags = r.u8()?;
        let _default_pan = r.u8()?; // bit 7 = disabled
        let length = r.u32_le()?;
        let loop_start = r.u32_le()?;
        let loop_end = r.u32_le()?;
        let c5_speed = r.u32_le()?;
        let susloop_start = r.u32_le()?;
        let susloop_end = r.u32_le()?;
        let sample_pointer = r.u32_le()?;
        let vibrato_speed = r.u8()?;
        let vibrato_depth = r.u8()?;
        let vibrato_sweep = r.u8()?;
        let vibrato_wave = r.u8()?;

        let bits16           = flags & 0x02 != 0;
        let stereo           = flags & 0x04 != 0;
        let compressed       = flags & 0x08 != 0;
        let loop_active      = flags & 0x10 != 0;
        let sustain_loop_active = flags & 0x20 != 0;
        let pingpong         = flags & 0x40 != 0;
        let sustain_pingpong = flags & 0x80 != 0;

        Ok(Self {
            dos_filename,
            global_vol,
            flags,
            default_vol,
            name,
            length,
            loop_start,
            loop_end,
            c5_speed,
            susloop_start,
            susloop_end,
            sample_pointer,
            vibrato_speed,
            vibrato_depth,
            vibrato_sweep,
            vibrato_wave,
            bits16,
            stereo,
            loop_active,
            sustain_loop_active,
            pingpong,
            sustain_pingpong,
            compressed,
            convert_flags,
        })
    }

    /// Best display name for this sample.
    pub fn display_name(&self) -> &str {
        if !self.name.trim().is_empty() {
            &self.name
        } else {
            &self.dos_filename
        }
    }

    /// Decompress or extract raw PCM data from the IT file bytes.
    /// Returns Vec<i16> (mono) or Vec<i16> (interleaved stereo).
    pub fn extract_pcm<'a>(&self, data: &'a [u8]) -> Result<Vec<i16>> {
        let ptr = self.sample_pointer as usize;
        if self.length == 0 || ptr == 0 {
            return Ok(Vec::new());
        }

        // Bit 2 of convert_flags: IT 2.15 double-delta encoding (vs IT 2.14 single-delta)
        let is_it215 = self.convert_flags & 0x04 != 0;
        if self.compressed {
            if self.bits16 {
                decompress_compressed_16bit(data, ptr, self.length as usize, self.stereo, is_it215)
            } else {
                decompress_compressed_8bit(data, ptr, self.length as usize, self.stereo, is_it215)
            }
        } else {
            extract_raw_pcm(data, ptr, self.length as usize, self.bits16, self.stereo)
        }
    }
}

// ---------------------------------------------------------------------------
// Instrument
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Instrument {
    pub dos_filename: String,
    pub new_note_action: u8,
    pub duplicate_check_type: u8,
    pub duplicate_check_action: u8,
    pub fadeout: u16,
    pub pitch_pan_separation: u8,
    pub pitch_pan_center: u8,
    pub global_vol: u8,
    pub default_pan: u8,
    pub random_vol: u8,
    pub random_pan: u8,
    pub name: String,
    pub filter_cutoff: u8,
    pub filter_resonance: u8,
    /// 120 entries of (note_transpose, sample_index_1based)
    pub note_sample_table: [(u8, u8); 120],
    pub env_vol: Envelope,
    pub env_pan: Envelope,
    pub env_pitch: Envelope,
}

impl Instrument {
    fn parse(data: &[u8], ptr: u32) -> Result<Self> {
        let ptr = ptr as usize;
        let mut r = Reader::new(data);
        r.seek(ptr);
        r.magic_check(b"IMPI")?;

        let dos_filename = r.string(12)?;
        r.skip(1)?; // padding
        let new_note_action = r.u8()?;
        let duplicate_check_type = r.u8()?;
        let duplicate_check_action = r.u8()?;
        let fadeout = r.u16_le()?;
        let pitch_pan_separation = r.u8()?;
        let pitch_pan_center = r.u8()?;
        let global_vol = r.u8()?;
        let default_pan = r.u8()?;
        let random_vol = r.u8()?;
        let random_pan = r.u8()?;
        let _cwtv = r.u16_le()?;
        let _num_samples = r.u8()?;
        r.skip(1)?; // padding
        let name = r.string(26)?;
        let filter_cutoff = r.u8()?;
        let filter_resonance = r.u8()?;
        let _midi_chan = r.u8()?;
        let _midi_prog = r.u8()?;
        let _midi_bank = r.u16_le()?;

        let mut note_sample_table = [(0u8, 0u8); 120];
        for entry in &mut note_sample_table {
            let note = r.u8()?;
            let samp = r.u8()?;
            *entry = (note, samp);
        }

        let env_vol = Envelope::parse(&mut r)?;
        let env_pan = Envelope::parse(&mut r)?;
        let env_pitch = Envelope::parse(&mut r)?;

        Ok(Self {
            dos_filename,
            new_note_action,
            duplicate_check_type,
            duplicate_check_action,
            fadeout,
            pitch_pan_separation,
            pitch_pan_center,
            global_vol,
            default_pan,
            random_vol,
            random_pan,
            name,
            filter_cutoff,
            filter_resonance,
            note_sample_table,
            env_vol,
            env_pan,
            env_pitch,
        })
    }

    pub fn display_name(&self) -> &str {
        if !self.name.trim().is_empty() {
            &self.name
        } else {
            &self.dos_filename
        }
    }

    /// New note action: 0=cut, 1=continue, 2=note-off, 3=fade
    pub fn nna_name(&self) -> &'static str {
        match self.new_note_action {
            0 => "cut",
            1 => "continue",
            2 => "note-off",
            3 => "fade",
            _ => "cut",
        }
    }

    /// Filter cutoff is only active when bit 7 is set.
    pub fn active_filter_cutoff(&self) -> Option<u8> {
        if self.filter_cutoff & 0x80 != 0 {
            Some(self.filter_cutoff & 0x7F)
        } else {
            None
        }
    }

    pub fn active_filter_resonance(&self) -> Option<u8> {
        if self.filter_resonance & 0x80 != 0 {
            Some(self.filter_resonance & 0x7F)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Song / top-level
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Song {
    pub title: String,
    pub num_orders: u16,
    pub num_instruments: u16,
    pub num_samples: u16,
    pub num_patterns: u16,
    pub flags: u16,
    pub global_vol: u8,
    pub mv: u8,
    pub speed: u8,
    pub tempo: u8,
    pub instruments: Vec<Instrument>,
    pub samples: Vec<Sample>,
    /// Raw file bytes (needed for PCM extraction)
    raw: Vec<u8>,
}

impl Song {
    /// Returns true if the file uses instrument mode (flag bit 2).
    pub fn uses_instruments(&self) -> bool {
        self.flags & 0x04 != 0
    }

    pub fn load(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data);

        r.magic_check(b"IMPM")?;

        let title = r.string(26)?;
        let _hilight_minor = r.u8()?;
        let _hilight_major = r.u8()?;
        let num_orders = r.u16_le()?;
        let num_instruments = r.u16_le()?;
        let num_samples = r.u16_le()?;
        let num_patterns = r.u16_le()?;
        let _cwtv = r.u16_le()?;
        let _cmwt = r.u16_le()?;
        let flags = r.flags16()?;
        let _special = r.u16_le()?;
        let global_vol = r.u8()?;
        let mv = r.u8()?;
        let speed = r.u8()?;
        let tempo = r.u8()?;
        r.skip(2)?; // sep, pwd
        let _msglength = r.u16_le()?;
        let _msgoffset = r.u32_le()?;
        r.skip(4)?; // reserved

        r.skip(64)?; // l_chnpan[64]
        r.skip(64)?; // l_chnvol[64]

        // Order list
        r.skip(num_orders as usize)?;

        // Instrument pointers
        let mut inst_ptrs = Vec::with_capacity(num_instruments as usize);
        for _ in 0..num_instruments {
            inst_ptrs.push(r.u32_le()?);
        }

        // Sample pointers
        let mut samp_ptrs = Vec::with_capacity(num_samples as usize);
        for _ in 0..num_samples {
            samp_ptrs.push(r.u32_le()?);
        }

        // Parse all instruments
        let instruments: Result<Vec<_>> = inst_ptrs
            .iter()
            .map(|&ptr| Instrument::parse(&data, ptr))
            .collect();
        let instruments = instruments?;

        // Parse all samples
        let samples: Result<Vec<_>> = samp_ptrs
            .iter()
            .map(|&ptr| Sample::parse(&data, ptr))
            .collect();
        let samples = samples?;

        Ok(Self {
            title,
            num_orders,
            num_instruments,
            num_samples,
            num_patterns,
            flags,
            global_vol,
            mv,
            speed,
            tempo,
            instruments,
            samples,
            raw: data,
        })
    }

    pub fn load_file(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::load(data)
    }

    pub fn raw_data(&self) -> &[u8] {
        &self.raw
    }
}

// ---------------------------------------------------------------------------
// PCM decompression
// ---------------------------------------------------------------------------

/// Extract raw (uncompressed) PCM from the file.
fn extract_raw_pcm(
    data: &[u8],
    ptr: usize,
    length: usize,
    bits16: bool,
    stereo: bool,
) -> Result<Vec<i16>> {
    let channels = if stereo { 2 } else { 1 };
    let total_samples = length * channels;

    if bits16 {
        let byte_len = total_samples * 2;
        ensure!(ptr + byte_len <= data.len(), "raw 16-bit sample data out of range");
        let mut out = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let b0 = data[ptr + i * 2];
            let b1 = data[ptr + i * 2 + 1];
            out.push(i16::from_le_bytes([b0, b1]));
        }
        Ok(out)
    } else {
        let byte_len = total_samples;
        ensure!(ptr + byte_len <= data.len(), "raw 8-bit sample data out of range");
        let mut out = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            // IT 8-bit samples are unsigned; convert to signed i16.
            let byte = data[ptr + i];
            let signed = (byte as i16) - 128;
            out.push(signed * 256);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// IT sample decompression (IT 2.14 / IT 2.15)
//
// Algorithm ported from xmodits-lib (B0ney, MPLv2), which itself references
// itsex.c (nicolasgramlich) and libmodplug (Konstanty).
//
// Blocks: 8-bit uses 0x8000-sample blocks, 16-bit uses 0x4000-sample blocks.
// Each block is preceded by a u16 LE byte-count of compressed data.
// IT 2.14: single-delta (output d1).  IT 2.15: double-delta (output d2).
// ---------------------------------------------------------------------------

/// LSB-first bit reader over a single compressed block.
struct BitReader<'a> {
    buf: &'a [u8],
    blk_index: usize,  // current byte index within buf
    bitbuf: u32,       // current byte's bits
    bitnum: u8,        // bits remaining in bitbuf
    block_offset: usize, // offset to the next block header in buf
}

impl<'a> BitReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, blk_index: 0, bitbuf: 0, bitnum: 0, block_offset: 0 }
    }

    /// Advance to the next block (reads the 2-byte block-size header).
    fn read_next_block(&mut self) -> bool {
        if self.block_offset + 2 > self.buf.len() { return false; }
        let block_size = u16::from_le_bytes([
            self.buf[self.block_offset],
            self.buf[self.block_offset + 1],
        ]) as usize;
        self.blk_index = self.block_offset + 2;
        if self.blk_index >= self.buf.len() { return false; }
        self.bitbuf = self.buf[self.blk_index] as u32;
        self.bitnum = 8;
        self.block_offset += block_size + 2;
        true
    }

    fn offset(&self) -> usize { self.block_offset }

    fn read_bits(&mut self, n: u8) -> Option<u32> {
        if n == 0 { return Some(0); }
        let mut value: u32 = 0;
        for _ in 0..n {
            if self.bitnum == 0 {
                self.blk_index += 1;
                if self.blk_index >= self.buf.len() { return None; }
                self.bitbuf = self.buf[self.blk_index] as u32;
                self.bitnum = 8;
            }
            value >>= 1;
            value |= self.bitbuf << 31;
            self.bitbuf >>= 1;
            self.bitnum -= 1;
        }
        Some(value >> (32 - n))
    }
}

// ---- 8-bit decompressor ---------------------------------------------------

fn decompress_compressed_8bit(
    data: &[u8],
    ptr: usize,
    length: usize,
    stereo: bool,
    is_it215: bool,
) -> Result<Vec<i16>> {
    let buf = &data[ptr..];
    let mut out = Vec::with_capacity(if stereo { length * 2 } else { length });
    let offset = decompress_8bit_inner(buf, length as u32, is_it215, &mut out)?;
    if stereo {
        // Right channel immediately follows left channel's blocks.
        decompress_8bit_inner(&buf[offset..], length as u32, is_it215, &mut out)?;
        // Deinterleave: L samples fill [0..length], R samples fill [length..length*2].
        // We need interleaved [L0,R0, L1,R1, ...].
        let (left, right) = out.split_at(length);
        let mut interleaved = Vec::with_capacity(length * 2);
        for i in 0..length {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }
        return Ok(interleaved);
    }
    Ok(out)
}

fn decompress_8bit_inner(
    buf: &[u8],
    mut len: u32,
    it215: bool,
    dest: &mut Vec<i16>,
) -> Result<usize> {
    let mut br = BitReader::new(buf);

    while len != 0 {
        if !br.read_next_block() { break; }

        let blklen: u16 = if len < 0x8000 { len as u16 } else { 0x8000 };
        let mut blkpos: u16 = 0;
        let mut width: u8 = 9;
        let mut d1: i8 = 0;
        let mut d2: i8 = 0;

        while blkpos < blklen {
            if width > 9 { break; }
            let value = match br.read_bits(width) { Some(v) => v as u16, None => break };

            if width < 7 {
                // Method 1: 1-6 bits
                if value == (1 << (width - 1)) as u16 {
                    let nb = match br.read_bits(3) { Some(v) => v as u8, None => break } + 1;
                    width = if nb < width { nb } else { nb + 1 };
                    continue;
                }
            } else if width < 9 {
                // Method 2: 7-8 bits
                let border = (0xffu16 >> (9 - width)).wrapping_sub(4);
                if value > border && value <= border + 8 {
                    let nb = (value - border) as u8;
                    width = if nb < width { nb } else { nb + 1 };
                    continue;
                }
            } else {
                // Method 3: 9 bits
                if (value & 0x100) != 0 {
                    width = ((value + 1) & 0xff) as u8;
                    continue;
                }
            }

            // Sign-extend to 8 bits
            let sample_value: i8 = if width < 8 {
                let shift = 8 - width;
                ((value << shift) as i8) >> shift
            } else {
                value as i8
            };

            d1 = d1.wrapping_add(sample_value);
            d2 = d2.wrapping_add(d1);
            let s = if it215 { d2 } else { d1 };
            dest.push((s as i16) * 256);
            blkpos += 1;
        }

        len -= blklen as u32;
    }

    Ok(br.offset())
}

// ---- 16-bit decompressor --------------------------------------------------

fn decompress_compressed_16bit(
    data: &[u8],
    ptr: usize,
    length: usize,
    stereo: bool,
    is_it215: bool,
) -> Result<Vec<i16>> {
    let buf = &data[ptr..];
    let mut out = Vec::with_capacity(if stereo { length * 2 } else { length });
    let offset = decompress_16bit_inner(buf, length as u32, is_it215, &mut out)?;
    if stereo {
        decompress_16bit_inner(&buf[offset..], length as u32, is_it215, &mut out)?;
        let (left, right) = out.split_at(length);
        let mut interleaved = Vec::with_capacity(length * 2);
        for i in 0..length {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }
        return Ok(interleaved);
    }
    Ok(out)
}

fn decompress_16bit_inner(
    buf: &[u8],
    mut len: u32,
    it215: bool,
    dest: &mut Vec<i16>,
) -> Result<usize> {
    let mut br = BitReader::new(buf);

    while len != 0 {
        if !br.read_next_block() { break; }

        let blklen: u16 = if len < 0x4000 { len as u16 } else { 0x4000 };
        let mut blkpos: u16 = 0;
        let mut width: u8 = 17;
        let mut d1: i16 = 0;
        let mut d2: i16 = 0;

        while blkpos < blklen {
            if width > 17 { break; }
            let value = match br.read_bits(width) { Some(v) => v, None => break };

            if width < 7 {
                // Method 1: 1-6 bits
                if value == 1u32 << (width - 1) {
                    let nb = match br.read_bits(4) { Some(v) => v as u8, None => break } + 1;
                    width = if nb < width { nb } else { nb + 1 };
                    continue;
                }
            } else if width < 17 {
                // Method 2: 7-16 bits
                let border = (0xffffu32 >> (17 - width)).wrapping_sub(8);
                if value > border && value <= border + 16 {
                    let nb = (value - border) as u8;
                    width = if nb < width { nb } else { nb + 1 };
                    continue;
                }
            } else {
                // Method 3: 17 bits
                if (value & 0x10000) != 0 {
                    width = ((value + 1) & 0xff) as u8;
                    continue;
                }
            }

            // Sign-extend to 16 bits
            let sample_value: i16 = if width < 16 {
                let shift = 16 - width;
                ((value << shift) as i16) >> shift
            } else {
                value as i16
            };

            d1 = d1.wrapping_add(sample_value);
            d2 = d2.wrapping_add(d1);
            dest.push(if it215 { d2 } else { d1 });
            blkpos += 1;
        }

        len -= blklen as u32;
    }

    Ok(br.offset())
}
