/// WAV file writer — MPC-compatible format.
///
///   fmt  (16 bytes, PCM)
///   atem (JSON: slice/loop metadata)
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

/// Write a 16-bit PCM WAV in MPC-compatible format (fmt + atem + meta + data).
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
    let last_frame = num_frames.saturating_sub(1);

    // Build atem JSON -------------------------------------------------------
    let loop_start = loop_info.filter(|l| l.active).map(|l| l.start).unwrap_or(0);
    let loop_mode: u32 = match loop_info {
        Some(l) if l.active && l.pingpong => 2,
        Some(l) if l.active => 1,
        _ => 0,
    };

    let mut atem_json = format!(
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

    // MPC's RIFF parser doesn't skip the padding byte on odd-size chunks, so
    // it misreads all subsequent chunk offsets. Ensure the JSON is always an
    // even number of bytes so no padding byte is ever needed.
    if atem_json.len() % 2 != 0 {
        atem_json.push(' ');
    }
    let atem_bytes = atem_json.as_bytes();

    // Chunk sizes -----------------------------------------------------------
    let data_bytes = (samples.len() * 2) as u32;
    // RIFF payload = "WAVE"(4) + fmt(8+16) + atem(8+len) + meta(8+4) + data(8+data_bytes)
    // atem_bytes.len() is always even so no padding byte is written.
    let riff_size: u32 = 4
        + (8 + 16)
        + (8 + atem_bytes.len() as u32)
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

    // atem chunk (always even-length, no padding byte needed)
    writer.write_all(b"atem")?;
    writer.write_all(&(atem_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(atem_bytes)?;

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
