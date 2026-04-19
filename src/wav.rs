/// WAV file writer — authentic MPC format.
///
/// Authentic MPC projects use 16-bit signed PCM (format 1) at the sample's
/// native rate, channel count preserved, with three MPC-specific metadata
/// chunks between `fmt` and `data`:
///
///   fmt  (16 bytes, PCM)
///   atem (JSON: slice/loop metadata)
///   smpl (94 bytes: standard 60-byte smpl + 34-byte Akai extension)
///   meta (4 bytes: zeroed)
///   data (16-bit signed PCM samples, LE)

use anyhow::Result;
use std::io::Write;

/// Optional loop metadata forwarded from the IT sample header.
pub struct LoopInfo {
    /// True if sample has an active forward or ping-pong loop.
    pub active: bool,
    /// Loop start, in sample frames (inclusive).
    pub start: u32,
    /// Loop end, in sample frames (exclusive, as stored in IMPS).
    pub end: u32,
    /// Ping-pong (alternating) loop if true; forward if false.
    pub pingpong: bool,
}

/// Write a 16-bit PCM WAV in MPC's native format (fmt + atem + smpl + meta + data).
///
/// `samples`  — flat i16 PCM: mono=[s0,s1,…], stereo=[L0,R0,L1,R1,…]
/// `num_channels` — 1 or 2
/// `sample_rate` — native sample rate in Hz
/// `loop_info` — loop params derived from the IT sample header (None = one-shot)
pub fn write_wav<W: Write>(
    writer: &mut W,
    samples: &[i16],
    num_channels: u16,
    sample_rate: u32,
    loop_info: Option<&LoopInfo>,
) -> Result<()> {
    let num_frames = (samples.len() / num_channels.max(1) as usize) as u32;
    let last_frame = num_frames.saturating_sub(1); // atem "End" and smpl loop end

    // Build atem JSON -------------------------------------------------------
    let loop_start = loop_info.filter(|l| l.active).map(|l| l.start).unwrap_or(0);
    // LoopMode: 0=none/oneshot, 1=forward, 2=pingpong (based on observed values)
    let loop_mode: u32 = match loop_info {
        Some(l) if l.active && l.pingpong => 2,
        Some(l) if l.active => 1,
        _ => 0,
    };

    let atem_json = format!(
        "{{\n\
        \x20   \"version\": 1,\n\
        \x20   \"value0\": {{\n\
        \x20       \"defaultSlice\": {{\n\
        \x20           \"Start\": 0,\n\
        \x20           \"End\": {},\n\
        \x20           \"LoopStart\": {},\n\
        \x20           \"LoopMode\": {},\n\
        \x20           \"PulsePosition\": 0,\n\
        \x20           \"LoopCrossfadeLength\": 0,\n\
        \x20           \"LoopCrossfadeType\": 0,\n\
        \x20           \"TailLength\": 0.0,\n\
        \x20           \"TailLoopPosition\": 0.5\n\
        \x20       }},\n\
        \x20       \"numBars\": 2,\n\
        \x20       \"Num slices\": 0\n\
        \x20   }},\n\
        \x20   \"value1\": {{\n\
        \x20       \"version\": 1,\n\
        \x20       \"note\": \"C#\",\n\
        \x20       \"scale\": \"Major\"\n\
        \x20   }}\n\
        }}",
        last_frame, loop_start, loop_mode
    );
    let atem_bytes = atem_json.as_bytes();
    // Pad to even length (RIFF word-alignment)
    let atem_padded = atem_bytes.len() + (atem_bytes.len() % 2);

    // Build smpl chunk (94 bytes) -------------------------------------------
    // 36-byte standard header + 24-byte loop record + 34-byte Akai extension.
    let period = (1_000_000_000u64 / sample_rate as u64) as u32;
    const UNITY_NOTE: u32 = 60; // C5

    // smpl loop end: use IT loop_end-1 if looping, otherwise last_frame
    let smpl_loop_start = loop_info.filter(|l| l.active).map(|l| l.start).unwrap_or(0);
    let smpl_loop_end = loop_info
        .filter(|l| l.active && l.end > 0)
        .map(|l| l.end - 1)
        .unwrap_or(last_frame);

    let mut smpl = [0u8; 94];
    let write_u32 = |buf: &mut [u8], off: usize, v: u32| {
        buf[off..off+4].copy_from_slice(&v.to_le_bytes());
    };
    write_u32(&mut smpl,  0, 0x01000047); // manufacturer (Akai)
    write_u32(&mut smpl,  4, 94);         // product
    write_u32(&mut smpl,  8, period);     // sample period (ns)
    write_u32(&mut smpl, 12, UNITY_NOTE); // MIDI unity note
    write_u32(&mut smpl, 16, 0);          // MIDI pitch fraction
    write_u32(&mut smpl, 20, 25);         // SMPTE format
    write_u32(&mut smpl, 24, 0);          // SMPTE offset
    write_u32(&mut smpl, 28, 1);          // num sample loops
    write_u32(&mut smpl, 32, 0);          // sampler data size
    // Loop record (offset 36)
    write_u32(&mut smpl, 36, 0);               // cue point id
    write_u32(&mut smpl, 40, 0);               // type: 0 = forward
    write_u32(&mut smpl, 44, smpl_loop_start); // loop start
    write_u32(&mut smpl, 48, smpl_loop_end);   // loop end
    write_u32(&mut smpl, 52, 0);               // fraction
    write_u32(&mut smpl, 56, 0);               // play count (0 = infinite)
    // 34-byte Akai extension (offset 60)
    smpl[60] = 0x03;
    smpl[61] = 0x00;
    write_u32(&mut smpl, 62, UNITY_NOTE);      // unity note repeated
    // bytes 66-73: zeros
    write_u32(&mut smpl, 74, smpl_loop_end);   // loop end repeated
    // bytes 78-92: zeros
    smpl[93] = 0x3f;

    // Chunk sizes -----------------------------------------------------------
    let data_bytes = (samples.len() * 2) as u32;
    // RIFF payload = "WAVE"(4) + fmt(8+16) + atem(8+padded) + smpl(8+94) + meta(8+4) + data(8+data_bytes)
    let riff_size: u32 = 4
        + (8 + 16)
        + (8 + atem_padded as u32)
        + (8 + 94)
        + (8 + 4)
        + (8 + data_bytes);

    let block_align = num_channels * 2;
    let byte_rate = sample_rate * block_align as u32;

    // Write -----------------------------------------------------------------
    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    // fmt chunk
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;           // PCM
    writer.write_all(&num_channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?;          // 16-bit

    // atem chunk
    writer.write_all(b"atem")?;
    writer.write_all(&(atem_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(atem_bytes)?;
    if atem_bytes.len() % 2 != 0 {
        writer.write_all(&[0u8])?; // pad to word boundary
    }

    // smpl chunk
    writer.write_all(b"smpl")?;
    writer.write_all(&94u32.to_le_bytes())?;
    writer.write_all(&smpl)?;

    // meta chunk
    writer.write_all(b"meta")?;
    writer.write_all(&4u32.to_le_bytes())?;
    writer.write_all(&[0u8; 4])?;

    // data chunk
    writer.write_all(b"data")?;
    writer.write_all(&data_bytes.to_le_bytes())?;
    for &s in samples {
        writer.write_all(&s.to_le_bytes())?;
    }

    Ok(())
}
