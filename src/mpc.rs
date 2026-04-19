/// MPC project file generator (.xpm / .xal / .xpj).

use crate::it::{Instrument, Sample};

pub struct PadEntry {
    /// Display name (from IT instrument or sample)
    #[allow(dead_code)]
    pub name: String,
    /// Filename of the extracted WAV (stem used for SampleName)
    pub wav_filename: String,
    /// Loop active
    pub loop_active: bool,
    /// Loop start in samples
    pub loop_start: u32,
    /// Loop end in samples
    pub loop_end: u32,
    /// Ping-pong loop
    #[allow(dead_code)]
    pub pingpong: bool,
    /// Sustain loop active (IT only; no direct MPC equivalent)
    #[allow(dead_code)]
    pub sustain_loop_active: bool,
    /// Sustain loop start
    #[allow(dead_code)]
    pub sustain_loop_start: u32,
    /// Sustain loop end
    #[allow(dead_code)]
    pub sustain_loop_end: u32,
    /// Volume 0.0–1.0 (from default_vol × global_vol)
    pub volume: f32,
    /// Pan 0.0–1.0 (0.5 = centre; None = default)
    pub pan: Option<f32>,
    /// Coarse tune in semitones (from instrument-level pitch, or 0)
    pub tune_coarse: i32,
    /// Fine tune in cents (-100..100)
    pub tune_fine: i32,
    /// OneShot: false if looping, true if not looping
    #[allow(dead_code)]
    pub one_shot: bool,
    /// Polyphony (0 = programme default)
    pub polyphony: u32,
    /// NNA:  "cut" | "continue" | "note-off" | "fade"
    #[allow(dead_code)]
    pub nna: String,
    /// Fadeout: 0..=128 mapped to 0..=1.0 release time
    pub fadeout: u16,
    /// Root/base note (MIDI note, 0–127); 60 = C5
    pub root_note: u32,
}

impl PadEntry {
    /// Build from a raw sample (no instrument wrapper).
    pub fn from_sample(sample: &Sample, wav_filename: String) -> Self {
        let vol = (sample.default_vol as f32 / 64.0) * (sample.global_vol as f32 / 64.0);
        Self {
            name: sample.display_name().to_string(),
            wav_filename,
            loop_active: sample.loop_active,
            loop_start: sample.loop_start,
            loop_end: sample.loop_end,
            pingpong: sample.pingpong,
            sustain_loop_active: sample.sustain_loop_active,
            sustain_loop_start: sample.susloop_start,
            sustain_loop_end: sample.susloop_end,
            volume: vol.clamp(0.0, 1.0),
            pan: None,
            tune_coarse: 0,
            tune_fine: 0,
            one_shot: !sample.loop_active,
            polyphony: 0,
            nna: "cut".to_string(),
            fadeout: 0,
            root_note: 60,
        }
    }

    /// Build from an IT instrument, using the primary sample for loop data.
    pub fn from_instrument(
        inst: &Instrument,
        primary_sample: Option<&Sample>,
        wav_filename: String,
    ) -> Self {
        let (loop_active, loop_start, loop_end, pingpong, sustain_loop_active, sustain_loop_start, sustain_loop_end, root_note) =
            if let Some(s) = primary_sample {
                (s.loop_active, s.loop_start, s.loop_end, s.pingpong,
                 s.sustain_loop_active, s.susloop_start, s.susloop_end, 60u32)
            } else {
                (false, 0, 0, false, false, 0, 0, 60u32)
            };

        let vol = inst.global_vol as f32 / 128.0;

        let pan = if inst.default_pan < 64 {
            Some(inst.default_pan as f32 / 64.0)
        } else if inst.default_pan == 64 {
            Some(0.5)
        } else {
            None // 128 / > 64 with bit 7 = "no pan" / use default
        };

        // fadeout: IT 0..128 (actually 0..=128; higher = faster fade)
        // Use it to modulate the volume release time.  We map it to polyphony
        // mode: if NNA != cut and fadeout > 0, allow multiple voices.
        let poly = if inst.new_note_action != 0 && inst.fadeout > 0 { 4 } else { 0 };

        Self {
            name: inst.display_name().to_string(),
            wav_filename,
            loop_active,
            loop_start,
            loop_end,
            pingpong,
            sustain_loop_active,
            sustain_loop_start,
            sustain_loop_end,
            volume: vol.clamp(0.0, 1.0),
            pan,
            tune_coarse: 0,
            tune_fine: 0,
            one_shot: !loop_active,
            polyphony: poly,
            nna: inst.nna_name().to_string(),
            fadeout: inst.fadeout,
            root_note,
        }
    }
}

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// .xpm generator
// ---------------------------------------------------------------------------

/// Generate the MPC Drum Program XML (.xpm) from a list of pad entries.
///
/// IT loop points are mapped to the Layer's Loop fields.
/// Fadeout is approximated via the VolumeRelease time.
/// NNA "fade" mode uses a non-zero release time and Polyphony > 0.
pub fn generate_xpm(program_name: &str, pads: &[PadEntry]) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n");
    xml.push_str("<MPCVObject>\n");
    xml.push_str("  <Version>\n");
    xml.push_str("    <File_Version>1.7</File_Version>\n");
    xml.push_str("    <Application>MPC-V</Application>\n");
    xml.push_str("    <Application_Version>2.0</Application_Version>\n");
    xml.push_str("    <Platform>Windows</Platform>\n");
    xml.push_str("  </Version>\n");
    xml.push_str("  <Program type=\"Drum\">\n");
    xml.push_str(&format!("    <Name>{}</Name>\n", xml_escape(program_name)));

    // ProgramPads JSON blob
    xml.push_str("    <ProgramPads>{\n");
    xml.push_str("    &quot;ProgramPads&quot;: {\n");
    xml.push_str("        &quot;Universal&quot;: {\n");
    xml.push_str("            &quot;value0&quot;: true\n");
    xml.push_str("        },\n");
    xml.push_str("        &quot;Type&quot;: {\n");
    xml.push_str("            &quot;value0&quot;: 1\n");
    xml.push_str("        },\n");
    xml.push_str("        &quot;universalPad&quot;: 32512,\n");
    xml.push_str("        &quot;pads&quot;: {\n");
    for i in 0..128 {
        let comma = if i < 127 { "," } else { "" };
        xml.push_str(&format!("            &quot;value{}&quot;: 0{}\n", i, comma));
    }
    xml.push_str("        },\n");
    xml.push_str("        &quot;UnusedPads&quot;: {\n");
    xml.push_str("            &quot;value0&quot;: 1\n");
    xml.push_str("        }\n");
    xml.push_str("    }\n");
    xml.push_str("}</ProgramPads>\n");

    // Program-level defaults
    xml.push_str("    <AudioRoute>\n");
    xml.push_str("      <AudioRoute>2</AudioRoute>\n");
    xml.push_str("      <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
    xml.push_str("      <InsertsEnabled>True</InsertsEnabled>\n");
    xml.push_str("    </AudioRoute>\n");
    xml.push_str("    <Send1>0.000000</Send1>\n");
    xml.push_str("    <Send2>0.000000</Send2>\n");
    xml.push_str("    <Send3>0.000000</Send3>\n");
    xml.push_str("    <Send4>0.000000</Send4>\n");
    xml.push_str("    <Volume>0.707946</Volume>\n");
    xml.push_str("    <Mute>False</Mute>\n");
    xml.push_str("    <Pan>0.500000</Pan>\n");
    xml.push_str("    <Pitch>0.000000</Pitch>\n");
    xml.push_str("    <TuneCoarse>0</TuneCoarse>\n");
    xml.push_str("    <TuneFine>0</TuneFine>\n");
    xml.push_str("    <Mono>False</Mono>\n");
    xml.push_str("    <Program_Polyphony>0</Program_Polyphony>\n");

    xml.push_str("    <Instruments>\n");

    for i in 0..128usize {
        xml.push_str(&format!("      <Instrument number=\"{}\">\n", i + 1));
        xml.push_str("        <AudioRoute>\n");
        xml.push_str("          <AudioRoute>0</AudioRoute>\n");
        xml.push_str("          <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
        xml.push_str("          <InsertsEnabled>True</InsertsEnabled>\n");
        xml.push_str("        </AudioRoute>\n");
        xml.push_str("        <Send1>0.000000</Send1>\n");
        xml.push_str("        <Send2>0.000000</Send2>\n");
        xml.push_str("        <Send3>0.000000</Send3>\n");
        xml.push_str("        <Send4>0.000000</Send4>\n");

        if let Some(pad) = pads.get(i) {
            let pan_val = pad.pan.unwrap_or(0.5);
            xml.push_str(&format!("        <Volume>{:.6}</Volume>\n", pad.volume));
            xml.push_str("        <Mute>False</Mute>\n");
            xml.push_str(&format!("        <Pan>{:.6}</Pan>\n", pan_val));
            xml.push_str(&format!("        <TuneCoarse>{}</TuneCoarse>\n", pad.tune_coarse));
            xml.push_str(&format!("        <TuneFine>{}</TuneFine>\n", pad.tune_fine));
            xml.push_str("        <Mono>False</Mono>\n");
            xml.push_str(&format!("        <Polyphony>{}</Polyphony>\n", pad.polyphony));
        } else {
            xml.push_str("        <Volume>1.000000</Volume>\n");
            xml.push_str("        <Mute>False</Mute>\n");
            xml.push_str("        <Pan>0.500000</Pan>\n");
            xml.push_str("        <TuneCoarse>0</TuneCoarse>\n");
            xml.push_str("        <TuneFine>0</TuneFine>\n");
            xml.push_str("        <Mono>False</Mono>\n");
            xml.push_str("        <Polyphony>0</Polyphony>\n");
        }

        xml.push_str("        <FilterKeytrack>0.000000</FilterKeytrack>\n");
        xml.push_str("        <LowNote>0</LowNote>\n");
        xml.push_str("        <HighNote>127</HighNote>\n");
        xml.push_str("        <IgnoreBaseNote>False</IgnoreBaseNote>\n");
        xml.push_str("        <ZonePlay>1</ZonePlay>\n");
        xml.push_str("        <MuteGroup>0</MuteGroup>\n");
        xml.push_str("        <MuteTarget1>0</MuteTarget1>\n");
        xml.push_str("        <MuteTarget2>0</MuteTarget2>\n");
        xml.push_str("        <MuteTarget3>0</MuteTarget3>\n");
        xml.push_str("        <MuteTarget4>0</MuteTarget4>\n");
        xml.push_str("        <SimultTarget1>0</SimultTarget1>\n");
        xml.push_str("        <SimultTarget2>0</SimultTarget2>\n");
        xml.push_str("        <SimultTarget3>0</SimultTarget3>\n");
        xml.push_str("        <SimultTarget4>0</SimultTarget4>\n");

        // TriggerMode: 2 = "Note" (sustain/loop); 0 = "Note Off" (one-shot).
        // This is what MPC's "Pad Loop" button in Program Edit reflects.
        if let Some(pad) = pads.get(i) {
            let trigger_mode = if pad.loop_active { 2 } else { 0 };
            xml.push_str(&format!("        <TriggerMode>{}</TriggerMode>\n", trigger_mode));
        } else {
            xml.push_str("        <TriggerMode>0</TriggerMode>\n");
        }

        xml.push_str("        <FilterType>0</FilterType>\n");
        xml.push_str("        <Cutoff>1.000000</Cutoff>\n");
        xml.push_str("        <Resonance>0.000000</Resonance>\n");
        xml.push_str("        <FilterEnvAmt>0.000000</FilterEnvAmt>\n");
        xml.push_str("        <AfterTouchToFilter>0.000000</AfterTouchToFilter>\n");
        xml.push_str("        <VelocityToStart>0.000000</VelocityToStart>\n");
        xml.push_str("        <VelocityToFilterAttack>0.000000</VelocityToFilterAttack>\n");
        xml.push_str("        <VelocityToFilter>0.000000</VelocityToFilter>\n");
        xml.push_str("        <VelocityToFilterEnvelope>0.000000</VelocityToFilterEnvelope>\n");
        xml.push_str("        <FilterAttack>0.000000</FilterAttack>\n");
        xml.push_str("        <FilterDecay>0.050000</FilterDecay>\n");
        xml.push_str("        <FilterSustain>1.000000</FilterSustain>\n");
        xml.push_str("        <FilterRelease>0.000000</FilterRelease>\n");
        xml.push_str("        <FilterHold>0.000000</FilterHold>\n");
        xml.push_str("        <FilterDecayType>True</FilterDecayType>\n");
        xml.push_str("        <FilterADEnvelope>True</FilterADEnvelope>\n");
        xml.push_str("        <VolumeHold>0.000000</VolumeHold>\n");
        xml.push_str("        <VolumeDecayType>True</VolumeDecayType>\n");
        xml.push_str("        <VolumeADEnvelope>True</VolumeADEnvelope>\n");
        xml.push_str("        <VolumeAttack>0.000000</VolumeAttack>\n");
        xml.push_str("        <VolumeDecay>0.050000</VolumeDecay>\n");
        xml.push_str("        <VolumeSustain>1.000000</VolumeSustain>\n");

        if let Some(pad) = pads.get(i) {
            // Map IT fadeout (0..=128, higher = faster) to MPC release time (0..=10 s).
            // fadeout=0 means no fade; fadeout=128 means very fast fade.
            let release = if pad.fadeout > 0 {
                // Invert: faster fadeout → shorter release
                let release = 2.0 * (1.0 - (pad.fadeout as f32 / 128.0));
                release.max(0.01)
            } else {
                0.0
            };
            xml.push_str(&format!("        <VolumeRelease>{:.6}</VolumeRelease>\n", release));
        } else {
            xml.push_str("        <VolumeRelease>0.000000</VolumeRelease>\n");
        }

        xml.push_str("        <VelocityToPitch>0.000000</VelocityToPitch>\n");
        xml.push_str("        <VelocityToVolumeAttack>0.000000</VelocityToVolumeAttack>\n");
        xml.push_str("        <VelocitySensitivity>1.000000</VelocitySensitivity>\n");
        xml.push_str("        <VelocityToPan>0.000000</VelocityToPan>\n");
        xml.push_str("        <LFO LfoNum=\"0\">\n");
        xml.push_str("          <Type>Sine</Type>\n");
        xml.push_str("          <Rate>0.500000</Rate>\n");
        xml.push_str("          <Sync>0</Sync>\n");
        xml.push_str("          <Reset>False</Reset>\n");
        xml.push_str("          <LfoPitch>0.000000</LfoPitch>\n");
        xml.push_str("          <LfoCutoff>0.000000</LfoCutoff>\n");
        xml.push_str("          <LfoVolume>0.000000</LfoVolume>\n");
        xml.push_str("          <LfoPan>0.000000</LfoPan>\n");
        xml.push_str("        </LFO>\n");
        xml.push_str("        <WarpTempo>120.000000</WarpTempo>\n");
        xml.push_str("        <BpmLock>True</BpmLock>\n");
        xml.push_str("        <WarpEnable>False</WarpEnable>\n");
        xml.push_str("        <StretchPercentage>100</StretchPercentage>\n");
        xml.push_str("        <EditAllLayers>False</EditAllLayers>\n");
        xml.push_str("        <Layers>\n");

        if let Some(pad) = pads.get(i) {
            let sample_name = std::path::Path::new(&pad.wav_filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&pad.wav_filename);
            xml.push_str("          <Layer number=\"1\">\n");
            xml.push_str("            <Active>True</Active>\n");
            xml.push_str("            <Volume>1.000000</Volume>\n");
            xml.push_str("            <Pan>0.500000</Pan>\n");
            xml.push_str("            <Pitch>0.000000</Pitch>\n");
            xml.push_str("            <TuneCoarse>0</TuneCoarse>\n");
            xml.push_str("            <TuneFine>0</TuneFine>\n");
            xml.push_str("            <VelStart>0</VelStart>\n");
            xml.push_str("            <VelEnd>127</VelEnd>\n");
            xml.push_str("            <SampleStart>0</SampleStart>\n");
            xml.push_str("            <SampleEnd>0</SampleEnd>\n");
            xml.push_str(&format!("            <LoopStart>{}</LoopStart>\n", pad.loop_start));
            xml.push_str(&format!("            <LoopEnd>{}</LoopEnd>\n", pad.loop_end));
            xml.push_str("            <LoopCrossfadeLength>0</LoopCrossfadeLength>\n");
            xml.push_str("            <LoopTune>0</LoopTune>\n");
            xml.push_str("            <Mute>False</Mute>\n");
            xml.push_str(&format!("            <RootNote>{}</RootNote>\n", pad.root_note));
            xml.push_str("            <KeyTrack>False</KeyTrack>\n");
            xml.push_str(&format!(
                "            <SampleName>{}</SampleName>\n",
                xml_escape(sample_name)
            ));
            xml.push_str("            <SampleFile></SampleFile>\n");
            xml.push_str("            <SliceIndex>128</SliceIndex>\n");
            xml.push_str("            <Direction>0</Direction>\n"); // 0=forward; ping-pong is encoded in WAV atem chunk
            xml.push_str("            <Offset>0</Offset>\n");
            xml.push_str("            <SliceStart>0</SliceStart>\n");
            xml.push_str("            <SliceEnd>0</SliceEnd>\n");
            xml.push_str("            <SliceLoopStart>0</SliceLoopStart>\n");
            xml.push_str("            <SliceLoop>0</SliceLoop>\n");
            xml.push_str("          </Layer>\n");
        }

        xml.push_str("        </Layers>\n");
        xml.push_str("      </Instrument>\n");
    }

    xml.push_str("    </Instruments>\n");
    xml.push_str("  </Program>\n");
    xml.push_str("</MPCVObject>\n");
    xml
}

// ---------------------------------------------------------------------------
// .xal generator
// ---------------------------------------------------------------------------

pub fn generate_xal() -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n");
    xml.push_str("<MPCVObject>\n");
    xml.push_str("  <Version>\n");
    xml.push_str("    <File_Version>1.7</File_Version>\n");
    xml.push_str("    <Application>MPC-V</Application>\n");
    xml.push_str("    <Application_Version>2.0</Application_Version>\n");
    xml.push_str("    <Platform>Windows</Platform>\n");
    xml.push_str("  </Version>\n");
    xml.push_str("  <AllSeqSamps>\n");
    xml.push_str("    <Sequences>\n");
    xml.push_str("      <Count>128</Count>\n");
    xml.push_str("      <Sequence number=\"1\">\n");
    xml.push_str("        <Active>True</Active>\n");
    xml.push_str("        <Name>Sequence 01</Name>\n");
    xml.push_str("      </Sequence>\n");
    xml.push_str("    </Sequences>\n");
    xml.push_str("    <Songs>\n");
    xml.push_str("      <Count>32</Count>\n");
    for i in 1..=32 {
        xml.push_str(&format!("      <Song number=\"{}\">\n", i));
        xml.push_str("        <Name>(unnamed)</Name>\n");
        xml.push_str("        <TempoIgnore>False</TempoIgnore>\n");
        xml.push_str("      </Song>\n");
    }
    xml.push_str("    </Songs>\n");
    xml.push_str("  </AllSeqSamps>\n");
    xml.push_str("</MPCVObject>\n");
    xml
}

// ---------------------------------------------------------------------------
// .xpj generator
// ---------------------------------------------------------------------------

pub fn generate_xpj(
    project_name: &str,
    project_data_dir_name: &str,
    wav_filenames: &[String],
) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n");
    xml.push_str("<Project>\n");
    xml.push_str("  <Version>\n");
    xml.push_str("    <File_Version>1.7</File_Version>\n");
    xml.push_str("    <Application>MPC-V</Application>\n");
    xml.push_str("    <Application_Version>2.0</Application_Version>\n");
    xml.push_str("    <Platform>Windows</Platform>\n");
    xml.push_str("  </Version>\n");
    xml.push_str("  <ProductCode>1299211094</ProductCode>\n");
    xml.push_str("  <BPM>120.000000</BPM>\n");
    xml.push_str("  <MasterVolume>0.707946</MasterVolume>\n");
    xml.push_str("  <FileList>\n");

    xml.push_str(&format!(
        "    <File>.\\{}\\All Sequences &amp; Songs.xal</File>\n",
        xml_escape(project_data_dir_name)
    ));

    for wav in wav_filenames {
        xml.push_str(&format!(
            "    <File>.\\{}\\{}</File>\n",
            xml_escape(project_data_dir_name),
            xml_escape(wav)
        ));
    }

    xml.push_str(&format!(
        "    <File>.\\{}\\{}.xpm</File>\n",
        xml_escape(project_data_dir_name),
        xml_escape(project_name)
    ));

    xml.push_str("  </FileList>\n");
    xml.push_str("  <Mixer>\n");

    xml.push_str("    <Mixer.Input number=\"1\">\n");
    xml.push_str("      <Name>Input</Name>\n");
    xml.push_str("      <AudioRoute>\n");
    xml.push_str("        <AudioRoute>2</AudioRoute>\n");
    xml.push_str("        <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
    xml.push_str("        <InsertsEnabled>True</InsertsEnabled>\n");
    xml.push_str("      </AudioRoute>\n");
    xml.push_str("      <Send1>0.000000</Send1>\n");
    xml.push_str("      <Send2>0.000000</Send2>\n");
    xml.push_str("      <Send3>0.000000</Send3>\n");
    xml.push_str("      <Send4>0.000000</Send4>\n");
    xml.push_str("      <Volume>0.707946</Volume>\n");
    xml.push_str("      <Mute>False</Mute>\n");
    xml.push_str("      <Pan>0.500000</Pan>\n");
    xml.push_str("    </Mixer.Input>\n");

    for i in 1..=4 {
        xml.push_str(&format!("    <Mixer.Return number=\"{}\">\n", i));
        xml.push_str(&format!("      <Name>Return {}</Name>\n", i));
        xml.push_str("      <AudioRoute>\n");
        xml.push_str("        <AudioRoute>2</AudioRoute>\n");
        xml.push_str("        <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
        xml.push_str("        <InsertsEnabled>True</InsertsEnabled>\n");
        xml.push_str("      </AudioRoute>\n");
        xml.push_str("      <Send1>0.000000</Send1>\n");
        xml.push_str("      <Send2>0.000000</Send2>\n");
        xml.push_str("      <Send3>0.000000</Send3>\n");
        xml.push_str("      <Send4>0.000000</Send4>\n");
        xml.push_str("      <Volume>0.707946</Volume>\n");
        xml.push_str("      <Mute>False</Mute>\n");
        xml.push_str("      <Pan>0.500000</Pan>\n");
        xml.push_str("    </Mixer.Return>\n");
    }

    for i in 1..=8 {
        xml.push_str(&format!("    <Mixer.Submix number=\"{}\">\n", i));
        xml.push_str(&format!("      <Name>Submix {}</Name>\n", i));
        xml.push_str("      <AudioRoute>\n");
        xml.push_str("        <AudioRoute>2</AudioRoute>\n");
        xml.push_str("        <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
        xml.push_str("        <InsertsEnabled>True</InsertsEnabled>\n");
        xml.push_str("      </AudioRoute>\n");
        xml.push_str("      <Send1>0.000000</Send1>\n");
        xml.push_str("      <Send2>0.000000</Send2>\n");
        xml.push_str("      <Send3>0.000000</Send3>\n");
        xml.push_str("      <Send4>0.000000</Send4>\n");
        xml.push_str("      <Volume>0.707946</Volume>\n");
        xml.push_str("      <Mute>False</Mute>\n");
        xml.push_str("      <Pan>0.500000</Pan>\n");
        xml.push_str("    </Mixer.Submix>\n");
    }

    for i in 1..=16 {
        xml.push_str(&format!("    <Mixer.Output number=\"{}\">\n", i));
        xml.push_str(&format!("      <Name>Output {}</Name>\n", i));
        xml.push_str("      <AudioRoute>\n");
        xml.push_str("        <AudioRoute>2</AudioRoute>\n");
        xml.push_str("        <AudioRouteSubIndex>0</AudioRouteSubIndex>\n");
        xml.push_str("        <InsertsEnabled>True</InsertsEnabled>\n");
        xml.push_str("      </AudioRoute>\n");
        xml.push_str("      <Send1>0.000000</Send1>\n");
        xml.push_str("      <Send2>0.000000</Send2>\n");
        xml.push_str("      <Send3>0.000000</Send3>\n");
        xml.push_str("      <Send4>0.000000</Send4>\n");
        xml.push_str("      <Volume>0.707946</Volume>\n");
        xml.push_str("      <Mute>False</Mute>\n");
        xml.push_str("      <Pan>0.500000</Pan>\n");
        xml.push_str("    </Mixer.Output>\n");
    }

    xml.push_str("  </Mixer>\n");
    xml.push_str("</Project>\n");
    xml
}
