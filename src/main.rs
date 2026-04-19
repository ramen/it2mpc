mod it;
mod mpc;
mod wav;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Convert an Impulse Tracker (.IT) file into an MPC project.
///
/// Extracts all samples/instruments from the .IT file, writes them as WAV
/// files, and generates the MPC Drum Program / project files (.xpm / .xal / .xpj).
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the input .IT file
    input: PathBuf,

    /// Output directory (created if it doesn't exist)
    output: PathBuf,

    /// Project / program name (defaults to .IT filename stem)
    #[arg(short, long)]
    name: Option<String>,

    /// Use sample-only mode: ignore instrument headers even if the file has them
    #[arg(long)]
    samples_only: bool,

    /// Print a summary of the IT file without writing any output
    #[arg(long)]
    info: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let song = it::Song::load_file(&args.input)
        .with_context(|| format!("Failed to load IT file: {}", args.input.display()))?;

    let project_name = args.name.unwrap_or_else(|| {
        args.input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    if args.info {
        print_info(&song, &project_name);
        return Ok(());
    }

    let use_instruments = song.uses_instruments() && !args.samples_only;

    println!("Song: {}", song.title);
    println!("Instruments: {}", song.num_instruments);
    println!("Samples: {}", song.num_samples);
    println!(
        "Mode: {}",
        if use_instruments { "instrument" } else { "sample" }
    );

    let output_dir = &args.output;
    let project_data_dir_name = format!("{}_[ProjectData]", project_name);
    let project_data_dir = output_dir.join(&project_data_dir_name);
    std::fs::create_dir_all(&project_data_dir)
        .with_context(|| format!("Failed to create output directory: {}", project_data_dir.display()))?;

    let (pads, wav_filenames) = if use_instruments {
        build_pads_from_instruments(&song, &project_data_dir)?
    } else {
        build_pads_from_samples(&song, &project_data_dir)?
    };

    // Warn if we have more than 128 pads (MPC limit)
    if pads.len() > 128 {
        eprintln!(
            "Warning: {} entries found; only the first 128 will be used (MPC limit).",
            pads.len()
        );
    }

    // Generate and write .xpm
    let xpm_content = mpc::generate_xpm(&project_name, &pads);
    let xpm_path = project_data_dir.join(format!("{}.xpm", project_name));
    std::fs::write(&xpm_path, &xpm_content)
        .with_context(|| format!("Failed to write {}", xpm_path.display()))?;
    println!("Wrote: {}", xpm_path.display());

    // Generate and write .xal
    let xal_content = mpc::generate_xal();
    let xal_path = project_data_dir.join("All Sequences & Songs.xal");
    std::fs::write(&xal_path, &xal_content)
        .with_context(|| format!("Failed to write {}", xal_path.display()))?;
    println!("Wrote: {}", xal_path.display());

    // Generate and write .xpj
    let xpj_content = mpc::generate_xpj(&project_name, &project_data_dir_name, &wav_filenames);
    let xpj_path = output_dir.join(format!("{}.xpj", project_name));
    std::fs::write(&xpj_path, &xpj_content)
        .with_context(|| format!("Failed to write {}", xpj_path.display()))?;
    println!("Wrote: {}", xpj_path.display());

    println!("\nDone. Open {} in MPC software.", xpj_path.display());
    Ok(())
}

/// Build pads in instrument mode: one pad per IT instrument.
fn build_pads_from_instruments(
    song: &it::Song,
    project_data_dir: &Path,
) -> Result<(Vec<mpc::PadEntry>, Vec<String>)> {
    let mut pads = Vec::new();
    let mut wav_filenames = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for (inst_idx, inst) in song.instruments.iter().enumerate().take(128) {
        // Find the primary sample: scan the note-sample table for any entry
        // that maps to an actual sample (1-based index).
        // We pick the first valid sample in the range C0-B9 (notes 0..119).
        let primary_sample_idx: Option<usize> = inst
            .note_sample_table
            .iter()
            .find_map(|&(_note, samp_idx)| {
                if samp_idx > 0 && (samp_idx as usize) <= song.samples.len() {
                    Some(samp_idx as usize - 1)
                } else {
                    None
                }
            });

        let primary_sample = primary_sample_idx.map(|idx| &song.samples[idx]);

        // Extract WAV for the primary sample
        let wav_filename = if let Some(samp) = primary_sample {
            let raw_name = sanitize_name(inst.display_name(), inst_idx + 1);
            let unique_name = make_unique_name(&raw_name, &mut used_names);
            let wav_filename = format!("{}.wav", unique_name);
            let wav_path = project_data_dir.join(&wav_filename);
            extract_sample_wav(song, samp, &wav_path)?;
            println!(
                "  Instrument {:>3}: {} → {}",
                inst_idx + 1,
                inst.display_name(),
                wav_filename
            );
            wav_filename
        } else {
            eprintln!(
                "  Instrument {:>3}: {} has no samples — skipping WAV",
                inst_idx + 1,
                inst.display_name()
            );
            String::new()
        };

        if !wav_filename.is_empty() {
            wav_filenames.push(wav_filename.clone());
        }

        let pad = mpc::PadEntry::from_instrument(inst, primary_sample, wav_filename);
        pads.push(pad);
    }

    Ok((pads, wav_filenames))
}

/// Build pads in sample-only mode: one pad per IT sample.
fn build_pads_from_samples(
    song: &it::Song,
    project_data_dir: &Path,
) -> Result<(Vec<mpc::PadEntry>, Vec<String>)> {
    let mut pads = Vec::new();
    let mut wav_filenames = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for (samp_idx, samp) in song.samples.iter().enumerate().take(128) {
        let raw_name = sanitize_name(samp.display_name(), samp_idx + 1);
        let unique_name = make_unique_name(&raw_name, &mut used_names);
        let wav_filename = format!("{}.wav", unique_name);
        let wav_path = project_data_dir.join(&wav_filename);
        extract_sample_wav(song, samp, &wav_path)?;
        println!(
            "  Sample {:>3}: {} → {}",
            samp_idx + 1,
            samp.display_name(),
            wav_filename
        );
        wav_filenames.push(wav_filename.clone());

        let pad = mpc::PadEntry::from_sample(samp, wav_filename);
        pads.push(pad);
    }

    Ok((pads, wav_filenames))
}

/// Extract a single sample as a WAV file.
fn extract_sample_wav(song: &it::Song, samp: &it::Sample, path: &Path) -> Result<()> {
    if samp.length == 0 {
        return Ok(());
    }

    let pcm = samp
        .extract_pcm(song.raw_data())
        .with_context(|| format!("Failed to extract sample '{}'", samp.display_name()))?;

    if pcm.is_empty() {
        return Ok(());
    }

    let channels: u16 = if samp.stereo { 2 } else { 1 };
    let sample_rate = samp.c5_speed.max(1);

    let loop_info = wav::LoopInfo {
        active: samp.loop_active,
        start: samp.loop_start,
        end: samp.loop_end,
        pingpong: samp.pingpong,
    };

    let mut file = std::fs::File::create(path)
        .with_context(|| format!("Failed to create WAV file: {}", path.display()))?;
    wav::write_wav(&mut file, &pcm, channels, sample_rate, Some(&loop_info))
        .with_context(|| format!("Failed to write WAV: {}", path.display()))?;

    Ok(())
}

/// Sanitize an IT name for use as a filename stem, falling back to a numeric name.
fn sanitize_name(name: &str, fallback_num: usize) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return format!("sample_{:03}", fallback_num);
    }
    // Replace characters not safe for filenames
    let safe: String = trimmed
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c => c,
        })
        .collect();
    let safe = safe.trim_matches('.').trim().to_string();
    if safe.is_empty() {
        format!("sample_{:03}", fallback_num)
    } else {
        safe
    }
}

/// Ensure a name is unique within the used set, appending a counter if needed.
fn make_unique_name(name: &str, used: &mut HashSet<String>) -> String {
    let lower = name.to_lowercase();
    if !used.contains(&lower) {
        used.insert(lower);
        return name.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{} ({})", name, n);
        let lower = candidate.to_lowercase();
        if !used.contains(&lower) {
            used.insert(lower);
            return candidate;
        }
        n += 1;
    }
}

fn print_info(song: &it::Song, project_name: &str) {
    println!("Project name : {}", project_name);
    println!("Song title   : {}", song.title);
    println!("Instruments  : {}", song.num_instruments);
    println!("Samples      : {}", song.num_samples);
    println!("Mode         : {}", if song.uses_instruments() { "instrument" } else { "sample" });
    println!("Speed        : {}", song.speed);
    println!("Tempo        : {}", song.tempo);
    println!("Global vol   : {}", song.global_vol);

    if !song.instruments.is_empty() {
        println!("\nInstruments:");
        for (i, inst) in song.instruments.iter().enumerate() {
            println!(
                "  {:>3}  {:26}  NNA={:10}  fadeout={}  vol={}  pan={}",
                i + 1,
                inst.display_name(),
                inst.nna_name(),
                inst.fadeout,
                inst.global_vol,
                if inst.default_pan <= 64 {
                    format!("{}", inst.default_pan)
                } else {
                    "default".to_string()
                }
            );
        }
    }

    if !song.samples.is_empty() {
        println!("\nSamples:");
        for (i, samp) in song.samples.iter().enumerate() {
            println!(
                "  {:>3}  {:26}  len={:>7}  loop={} ({}-{}){}  {}Hz  {}bit{}",
                i + 1,
                samp.display_name(),
                samp.length,
                if samp.loop_active { "on " } else { "off" },
                samp.loop_start,
                samp.loop_end,
                if samp.pingpong { " pp" } else { "   " },
                samp.c5_speed,
                if samp.bits16 { 16 } else { 8 },
                if samp.stereo { " stereo" } else { "" }
            );
        }
    }
}
