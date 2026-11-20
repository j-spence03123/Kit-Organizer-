//! Classification tests covering common sample-pack naming conventions.
//! All names are synthetic; no commercial content is needed.

use super::*;
use crate::config::{default_config, CompiledConfig};

fn classifier() -> Classifier {
    Classifier::new(CompiledConfig::compile(&default_config()).unwrap())
}

/// `rel` is "<imported root>/<sub folders>/<file>".
fn run_with(c: &Classifier, rel: &str, audio: Option<&AudioMeta>) -> Classification {
    let parts: Vec<&str> = rel.split('/').collect();
    let (file, folders) = parts.split_last().unwrap();
    let folders: Vec<String> = folders.iter().map(|s| s.to_string()).collect();
    let kind = crate::filetypes::FileTypeSettings::default().detect(std::path::Path::new(file));
    c.classify(&ClassifyInput { file_name: file, folders: &folders, kind, audio })
}

fn run(rel: &str) -> Classification {
    run_with(&classifier(), rel, None)
}

#[track_caller]
fn expect(rel: &str, path: &str) -> Classification {
    let r = run(rel);
    assert_eq!(r.category_path, path, "{rel}\nevidence: {:#?}", r.evidence);
    r
}

#[test]
fn spec_examples() {
    expect("XLNT Sample Pack/DRUMS/ONE SHOTS/KICKS/XLNT_Kick_01.wav", "Kicks");
    expect("Pack/House_Kick_01.wav", "Kicks");
    expect("Pack/House_Kick_Loop_128BPM.wav", "Drum Loops/Kick Loops");
    expect("Pack/Piano_Melody_140BPM.wav", "Melodic Loops/Piano Loops");
    expect("Pack/Piano_C4.wav", "One-Shots/Keys/Pianos");
    expect("Pack/XLNT_Growl_F.wav", "Bass/Growls");
    expect("Pack/XLNT_Bass_Loop_150BPM.wav", "Bass/Bass Loops");
}

#[test]
fn reverse_crash_in_fx_folder_is_a_reverse_cymbal() {
    let r = expect("Pack/FX/Reverse Crash.wav", "Cymbals/Reverse Cymbals");
    assert!(r.alternatives.iter().any(|a| a.path.starts_with("FX")), "{:?}", r.alternatives);
}

#[test]
fn neuro_growl_prefers_head_noun_and_lists_alternative() {
    let r = expect("Pack/Bass/XLNT_Neuro_Growl_F.wav", "Bass/Growls");
    assert!(r.alternatives.iter().any(|a| a.path == "Bass/Neuros"), "{:?}", r.alternatives);
    assert_eq!(r.key.as_deref(), Some("F"));
    assert!(r.evidence.iter().any(|e| e.contains("Parent folder \"Bass\"")), "{:#?}", r.evidence);
}

#[test]
fn xlnt_style_bass_names() {
    expect("XLNT Riddim Vol 2/Bass/XLNT_Screech_03_F#.wav", "Bass/Screeches");
    expect("XLNT Riddim Vol 2/Bass/XLNT_Yoink_01_G.wav", "Bass/Yoinks");
    expect("XLNT Riddim Vol 2/Bass/XLNT_Riddim_Bass_07.wav", "Bass/Riddim Basses");
    expect("XLNT Riddim Vol 2/Bass/XLNT_Gunshot_Bass_02.wav", "Bass/Gunshots");
    expect("XLNT Riddim Vol 2/Bass/XLNT_Donk_04.wav", "Bass/Donks");
    expect("XLNT Riddim Vol 2/Bass/XLNT_Bass_12.wav", "Bass/Other Basses");
}

#[test]
fn kompany_style_bass_names() {
    expect("Kompany Bass Kit/Kompany_Bass_Wub_01.wav", "Bass/Wubs");
    expect("Kompany Bass Kit/Basses/Kompany_Metallic_Bass_03.wav", "Bass/Metallic Basses");
    expect("Kompany Bass Kit/Kompany_Reese_A.wav", "Bass/Reeses");
    expect("Kompany Bass Kit/Kompany_Wobble_Bass_150.wav", "Bass/Wobbles");
    expect("Kompany Bass Kit/Kompany_FM_Bass_09.wav", "Bass/FM Basses");
    expect("Kompany Bass Kit/Kompany_Glitch_Bass_02.wav", "Bass/Glitch Basses");
}

#[test]
fn black_octopus_style_names() {
    expect("BOS Tearout/BOS_TT_Bass_Loop_140_Fmin.wav", "Bass/Bass Loops");
    expect("BOS Tearout/Drums/BOS_TT_Kick_01.wav", "Kicks");
    expect("BOS Tearout/Drum Loops/BOS_TT_Top_Loop_140.wav", "Drum Loops/Top Loops");
    expect("BOS Tearout/Drum Loops/BOS_TT_Drum_Loop_140_Full.wav", "Drum Loops/Full Loops");
    expect("BOS Tearout/FX/BOS_TT_Riser_04.wav", "FX/Risers");
    expect("BOS Tearout/Vocals/BOS_TT_Vocal_Chop_Fm_01.wav", "Vocals/Vocal Chops");
    let r = expect("BOS Tearout/BOS_TT_Synth_Loop_140_Am.wav", "Melodic Loops/Synth Loops");
    assert_eq!(r.key.as_deref(), Some("Am"));
}

#[test]
fn conventional_drum_kit_names() {
    expect("Trap Kit/Snares/Snare 01.wav", "Snares");
    expect("Trap Kit/Claps/clap_hard.wav", "Claps");
    expect("Trap Kit/Hats/Closed Hat 3.wav", "Hi-Hats/Closed Hats");
    expect("Trap Kit/Hats/OH_05.wav", "Hi-Hats/Open Hats");
    expect("Trap Kit/Hi Hats/hat_2.wav", "Hi-Hats");
    expect("Trap Kit/Perc/Shaker_01.wav", "Percussion/Shakers");
    expect("Trap Kit/Perc/perc_weird_03.wav", "Percussion/Other Percussion");
    expect("Trap Kit/808s/808_Long_C.wav", "Bass/808s");
    expect("Trap Kit/Rims/Rimshot_2.wav", "Rims");
    expect("Trap Kit/Cymbals/Crash_01.wav", "Cymbals/Crashes");
    expect("Trap Kit/Cymbals/Ride_Bell.wav", "Cymbals/Rides");
    expect("Trap Kit/Toms/Floor Tom.wav", "Toms");
    expect("Trap Kit/Kick_Snare_Pattern_Loop_140bpm.wav", "Drum Loops/Snare Loops");
}

#[test]
fn case_and_punctuation_variations() {
    expect("P/KICK-01.WAV", "Kicks");
    expect("P/kick01.wav", "Kicks");
    expect("P/ClosedHat_04.wav", "Hi-Hats/Closed Hats");
    expect("P/Hi-Hat Open 2.wav", "Hi-Hats/Open Hats");
    expect("P/HiHat_Closed_9.wav", "Hi-Hats/Closed Hats");
    expect("P/open_hihat (wet).wav", "Hi-Hats/Open Hats");
    expect("P/snare  roll   01.wav", "Drum Loops/Drum Fills");
}

#[test]
fn abbreviations() {
    expect("P/BD_01.wav", "Kicks");
    expect("P/SD_Tight.wav", "Snares");
    expect("P/CHH_03.wav", "Hi-Hats/Closed Hats");
    expect("P/OHH_03.wav", "Hi-Hats/Open Hats");
    expect("P/CLP_1.wav", "Claps");
    expect("P/Crsh_2.wav", "Cymbals/Crashes");
}

#[test]
fn short_abbreviations_do_not_match_inside_words() {
    // "bd" inside "Abduction", "sd" inside "Wisdom" must not match.
    let r = run("P/Abduction_Wisdom.wav");
    assert_eq!(r.category_path, "Uncategorised");
    assert_eq!(r.confidence, 0.0);
}

#[test]
fn conflicting_folder_and_filename_prefers_filename() {
    let r = expect("P/Kicks/Snare_07.wav", "Snares");
    assert!(r.alternatives.iter().any(|a| a.path == "Kicks"));
    assert!(r.confidence < 0.8, "conflict should reduce confidence: {}", r.confidence);
}

#[test]
fn folder_only_evidence() {
    let r = expect("P/Drums/One Shots/Kicks/XLNT_01.wav", "Kicks");
    assert_eq!(r.method, "folder");
    assert!(r.confidence > 0.5 && r.confidence < 0.8, "{}", r.confidence);
}

#[test]
fn generic_folders_are_ignored() {
    let r = run("P/Samples/Audio/Misc/Thing_01.wav");
    assert_eq!(r.category_path, "Uncategorised");
}

#[test]
fn imported_root_is_weak_evidence() {
    // A pack called "Bass Pack" should not drag a kick into Bass.
    expect("Bass Pack/Kick_01.wav", "Kicks");
}

#[test]
fn loops_vs_one_shots() {
    expect("P/Loops/Hats/Hat_Groove_01.wav", "Drum Loops/Hi-Hat Loops");
    expect("P/Guitar_Loop_90.wav", "Melodic Loops/Guitar Loops");
    expect("P/Guitar_Pluck_C3.wav", "One-Shots/Plucks");
    expect("P/One Shots/Synth_120bpm_Chord.wav", "One-Shots/Chords");
    expect("P/Vocals/Vox_Loop_Am_128BPM.wav", "Vocals/Vocal Loops");
    expect("P/Breaks/Amen_Break_170bpm.wav", "Drum Loops/Breakbeats");
}

#[test]
fn bpm_alone_is_not_a_loop() {
    let r = expect("P/Lead_128BPM_C.wav", "One-Shots/Leads");
    assert_eq!(r.sample_type, SampleType::OneShot);
    assert_eq!(r.bpm, Some(128.0));
}

#[test]
fn long_duration_alone_is_not_a_loop() {
    let meta = AudioMeta { duration_secs: Some(12.0), ..Default::default() };
    let r = run_with(&classifier(), "P/Pad_Warm_C.wav", Some(&meta));
    assert_eq!(r.category_path, "One-Shots/Pads");
}

#[test]
fn acid_metadata_supports_loop_detection() {
    let meta = AudioMeta { duration_secs: Some(8.0), acid_one_shot: Some(false), acid_tempo: Some(140.0), ..Default::default() };
    let r = run_with(&classifier(), "P/Piano_Chords_Am.wav", Some(&meta));
    assert_eq!(r.category_path, "Melodic Loops/Chord Loops", "{:#?}", r.evidence);
    assert_eq!(r.bpm, Some(140.0));
    assert_eq!(r.bpm_source.as_deref(), Some("metadata"));
}

#[test]
fn file_types() {
    let r = expect("P/MIDI/Chords_Am.mid", "MIDI");
    assert_eq!(r.sample_type, SampleType::Midi);
    assert_eq!(r.confidence, 0.99);
    expect("P/Serum Presets/BASS - Growl.fxp", "Presets");
}

#[test]
fn fx_and_vocals() {
    expect("P/FX/Downlifter_02.wav", "FX/Downlifters");
    expect("P/FX/Impact_Big.wav", "FX/Impacts");
    expect("P/FX/Sub_Drop_01.wav", "FX/Impacts");
    expect("P/FX/White_Noise_Sweep_Up.wav", "FX/Sweeps");
    expect("P/FX/Weird_01.wav", "FX/Other FX");
    expect("P/Vocals/Acapella_Female_Am.wav", "Vocals/Acapellas");
    expect("P/Vocals/Adlib_Yeah.wav", "Vocals/Ad-libs");
    expect("P/Vocals/Vocal_Thing.wav", "Vocals/Other Vocals");
}

#[test]
fn melodic_one_shots() {
    expect("P/Rhodes_Chord_Cm.wav", "One-Shots/Chords");
    expect("P/Rhodes_C3.wav", "One-Shots/Keys/Electric Pianos");
    expect("P/Bell_Hit_G.wav", "One-Shots/Bells");
    expect("P/Strings_Long_D.wav", "One-Shots/Strings");
    expect("P/Arp_Loop_128bpm.wav", "Melodic Loops/Arpeggio Loops");
}

#[test]
fn confidence_is_evidence_based() {
    let strong = run("P/Kicks/Kick_01.wav").confidence;
    let filename_only = run("P/Kick_01.wav").confidence;
    let folder_only = run("P/Kicks/Thing_01.wav").confidence;
    assert!(strong > filename_only && filename_only > folder_only, "{strong} {filename_only} {folder_only}");
    assert!(strong <= 0.99);
}

#[test]
fn priorities_break_ties() {
    let mut cfg = default_config();
    fn bump(nodes: &mut [crate::config::CategoryNode], id: &str) {
        for n in nodes {
            if n.id == id {
                n.priority = 5;
            }
            bump(&mut n.children, id);
        }
    }
    bump(&mut cfg.categories, "bass/neuros");
    let c = Classifier::new(CompiledConfig::compile(&cfg).unwrap());
    assert_eq!(run_with(&c, "P/Bass/Neuro_Growl.wav", None).category_path, "Bass/Neuros");
}

#[test]
fn custom_aliases_apply() {
    let mut cfg = default_config();
    cfg.categories.iter_mut().find(|n| n.id == "kicks").unwrap().aliases.push("thump".into());
    let c = Classifier::new(CompiledConfig::compile(&cfg).unwrap());
    assert_eq!(run_with(&c, "P/Thump_01.wav", None).category_path, "Kicks");
}

#[test]
fn deterministic() {
    let a = run("P/FX/Reverse Crash.wav");
    for _ in 0..5 {
        assert_eq!(run("P/FX/Reverse Crash.wav"), a);
    }
}
