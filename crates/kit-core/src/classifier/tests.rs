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
    c.classify(&ClassifyInput { file_name: file, folders: &folders, kind, audio, hints: &[] })
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
    let r = expect("P/Lead_128BPM_01.wav", "One-Shots/Leads");
    assert_eq!(r.sample_type, SampleType::OneShot);
    assert_eq!(r.bpm, Some(128.0));
    // Tempo + key suggests a loop, but a short file is still a one-shot.
    let short = AudioMeta { duration_secs: Some(0.9), ..Default::default() };
    let r = run_with(&classifier(), "P/Lead_128BPM_C.wav", Some(&short));
    assert_eq!(r.category_path, "One-Shots/Leads", "{:#?}", r.evidence);
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
    assert_eq!(r.bpm_source.as_deref(), Some("ACID tag"));
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

#[test]
fn weak_pack_name_alone_falls_back_to_uncategorised() {
    let r = run("XLNT Riddim Vol 1/Misc/Thing_01.wav");
    assert_eq!(r.category_path, "Uncategorised");
    assert_eq!(r.alternatives.first().map(|a| a.path.as_str()), Some("Bass/Riddim Basses"));
}

#[test]
fn short_folder_names_are_strong_when_whole() {
    let r = expect("P/HH/Thing_01.wav", "Hi-Hats");
    // Folder-only evidence for a broad parent category: strong alias (not
    // halved), but still below the default review threshold.
    assert!(r.confidence > 0.4, "{}", r.confidence);
}

#[test]
fn loop_context_supports_instrument_loops() {
    let r = expect("XLNT/Melodic/Piano_Melody_140BPM.wav", "Melodic Loops/Piano Loops");
    assert!(r.confidence >= 0.6, "{} {:#?}", r.confidence, r.evidence);
    let r = expect("P/Drum Loops/Kick_Groove_120bpm.wav", "Drum Loops/Kick Loops");
    assert!(r.confidence >= 0.6, "{} {:#?}", r.confidence, r.evidence);
}

// ---------- smarter matching ----------

#[test]
fn typos_are_tolerated_only_when_nothing_matches_exactly() {
    let r = expect("P/Snere_01.wav", "Snares");
    assert!(r.evidence[0].contains("probable typo"), "{:#?}", r.evidence);
    expect("P/XLNT_Screetch_02.wav", "Bass/Screeches");
    // "Sweet" is one edit from "sweep", but "pad" matched exactly.
    expect("P/Sweet_Pad_C.wav", "One-Shots/Pads");
}

#[test]
fn compounds_are_split() {
    expect("P/growlbass_04.wav", "Bass/Growls");
    expect("P/kickroll_01.wav", "Kicks");
}

#[test]
fn vendor_prefixes_are_ignored() {
    // "TT" would otherwise be meaningless noise; "BOS" is a known vendor.
    let r = expect("BOS Pack/BOS_TT_Snare_03.wav", "Snares");
    assert!(!r.evidence.iter().any(|e| e.contains("\"bos\"")));
}

#[test]
fn context_dependent_aliases() {
    expect("P/Snare_Roll_01.wav", "Drum Loops/Drum Fills");
    // "roll" alone (no drum context) is not a fill.
    assert_eq!(run("P/Roll_Thing.wav").category_path, "Uncategorised");
    // "top" is a top loop only in a drum context.
    expect("P/Drum Loops/Top_140.wav", "Drum Loops/Top Loops");
    assert_eq!(run("P/Top_Shelf_01.wav").category_path, "Uncategorised");
}

#[test]
fn fm_is_not_f_major() {
    let r = expect("P/FM_Bass_03.wav", "Bass/FM Basses");
    assert_eq!(r.key, None);
}

// ---------- metadata ----------

#[test]
fn embedded_tags_are_evidence() {
    let meta = AudioMeta { title: Some("Deep Kick".into()), genre: Some("One Shot".into()), ..Default::default() };
    let r = run_with(&classifier(), "P/Sample_034.wav", Some(&meta));
    assert_eq!(r.category_path, "Kicks", "{:#?}", r.evidence);
    assert!(r.method.contains("tags"));
}

#[test]
fn tag_key_and_bpm() {
    let meta = AudioMeta { tag_key: Some("Am".into()), tag_bpm: Some(140.0), duration_secs: Some(6.9), ..Default::default() };
    let r = run_with(&classifier(), "P/Synth_Loop.wav", Some(&meta));
    assert_eq!(r.key.as_deref(), Some("Am"));
    assert_eq!(r.key_source.as_deref(), Some("tag"));
    assert_eq!(r.bpm, Some(140.0));
    assert_eq!(r.bpm_source.as_deref(), Some("tag"));
}

#[test]
fn default_root_note_is_not_a_key() {
    let c4 = AudioMeta { root_note: Some(60), ..Default::default() };
    assert_eq!(run_with(&classifier(), "P/Pluck_01.wav", Some(&c4)).key, None);
    let g = AudioMeta { root_note: Some(67), ..Default::default() };
    let r = run_with(&classifier(), "P/Pluck_01.wav", Some(&g));
    assert_eq!(r.key.as_deref(), Some("G"));
    assert_eq!(r.key_source.as_deref(), Some("sampler root note"));
}

// ---------- audio analysis ----------

fn features(f: crate::audio::AudioFeatures, secs: f64) -> AudioMeta {
    AudioMeta { duration_secs: Some(secs), features: Some(f), ..Default::default() }
}

fn kick_features() -> crate::audio::AudioFeatures {
    crate::audio::AudioFeatures {
        analysed_secs: 0.5,
        low_ratio: 0.8,
        attack_ms: 5.0,
        decay_ms: Some(300.0),
        onsets: 1,
        ..Default::default()
    }
}

#[test]
fn audio_classifies_nameless_files() {
    let r = run_with(&classifier(), "P/Sample_034.wav", Some(&features(kick_features(), 0.5)));
    assert_eq!(r.category_path, "Kicks", "{:#?}", r.evidence);
    assert!(r.confidence < 0.6, "audio alone should still be reviewed: {}", r.confidence);
    assert_eq!(r.sample_type, SampleType::OneShot);
}

#[test]
fn audio_supports_but_never_overrides_names() {
    let plain = run("P/Kick_01.wav").confidence;
    let supported = run_with(&classifier(), "P/Kick_01.wav", Some(&features(kick_features(), 0.5)));
    assert!(supported.confidence > plain, "{} vs {plain}", supported.confidence);
    // A "snare" that looks like a kick to the analysis stays a snare at full confidence.
    let snare = run_with(&classifier(), "P/Snare_01.wav", Some(&features(kick_features(), 0.5)));
    assert_eq!(snare.category_path, "Snares");
    assert_eq!(snare.confidence, run("P/Snare_01.wav").confidence);
    assert!(snare.evidence.iter().any(|e| e.contains("ignored because the names disagree")));
}

#[test]
fn audio_rhythm_supports_loops() {
    let f = crate::audio::AudioFeatures { analysed_secs: 8.0, onsets: 16, periodicity: 0.8, ..Default::default() };
    let r = run_with(&classifier(), "P/Drums/Hat_Pattern.wav", Some(&features(f, 8.0)));
    assert_eq!(r.category_path, "Drum Loops/Hi-Hat Loops", "{:#?}", r.evidence);
}

// ---------- learned rules ----------

fn learned(rules: Vec<(&str, &str, &str)>) -> Classifier {
    let cfg = CompiledConfig::compile(&default_config()).unwrap();
    let rules = rules
        .into_iter()
        .enumerate()
        .map(|(i, (example, folders, cat))| {
            let folders: Vec<String> = folders.split('/').map(String::from).collect();
            let p = learned::pattern_of(&cfg, example, &folders).unwrap();
            learned::LearnedRule {
                id: i as i64,
                kind: p.kind,
                tokens: p.tokens,
                scope: p.scope,
                category_id: cat.into(),
                example: example.into(),
                hits: 1,
                created_at: String::new(),
            }
        })
        .collect();
    Classifier::new(cfg).with_learned(rules)
}

#[test]
fn learned_patterns() {
    let cfg = CompiledConfig::compile(&default_config()).unwrap();
    let p = learned::pattern_of(&cfg, "XLNT_Shot_03_F.wav", &["XLNT Pack".into()]).unwrap();
    assert_eq!(p.tokens, ["shot"]);
    assert_eq!(p.scope.as_deref(), Some("xlnt"));
    let p = learned::pattern_of(&cfg, "XLNT_07.wav", &["XLNT Pack".into(), "Shots".into()]).unwrap();
    assert_eq!(p.kind, learned::RuleKind::Folder);
    assert_eq!(p.tokens, ["shots"]);
}

#[test]
fn corrections_apply_to_similar_files() {
    let c = learned(vec![("XLNT_Shot_03_F.wav", "XLNT Pack/Bass", "bass/bass-stabs")]);
    // Without learning, "shot" alone is only a one-shot hint.
    assert_ne!(run("XLNT Pack/Bass/XLNT_Shot_07_G.wav").category_path, "Bass/Bass Stabs");
    let r = run_with(&c, "XLNT Pack/Bass/XLNT_Shot_07_G.wav", None);
    assert_eq!(r.category_path, "Bass/Bass Stabs", "{:#?}", r.evidence);
    assert!(r.method.contains("learned"));
    assert!(r.confidence >= 0.8);
    // Elsewhere (different vendor) it is weaker but still applies.
    let other = run_with(&c, "Other Pack/Shot_2.wav", None);
    assert_eq!(other.category_path, "Bass/Bass Stabs");
    assert!(other.confidence < r.confidence);
}

#[test]
fn learned_rules_do_not_override_clear_names_elsewhere() {
    let c = learned(vec![("XLNT_Shot_03.wav", "XLNT Pack", "bass/bass-stabs")]);
    // "Kick Shot" in another pack: partial match only, kick wins.
    expect_with(&c, "Other Pack/Kick_Shot_01.wav", "Kicks");
}

#[track_caller]
fn expect_with(c: &Classifier, rel: &str, path: &str) {
    let r = run_with(c, rel, None);
    assert_eq!(r.category_path, path, "{rel}\nevidence: {:#?}", r.evidence);
}

// ---------- melody-kit naming ----------

fn long() -> AudioMeta {
    AudioMeta { duration_secs: Some(34.0), ..Default::default() }
}

#[test]
fn titled_melody_loops_with_key_and_bpm() {
    for name in [
        "Kits/Utopia Kit/cashier [133 BPM D minor] @someproducer.wav",
        "Kits/Utopia Kit/alien 155BPM (g4) [prod. somebody].wav",
        "Kits/Utopia Kit/goons [140 BPM E minor] @a @b.wav",
        "Kits/Utopia Kit/motion 140bpm f#min.wav",
    ] {
        let r = run_with(&classifier(), name, Some(&long()));
        assert_eq!(r.category_path, "Melodic Loops/Full Melody Loops", "{name}\n{:#?}", r.evidence);
        assert!(r.confidence >= 0.6, "{name}: {} {:#?}", r.confidence, r.evidence);
        assert!(r.key.is_some());
    }
}

#[test]
fn melody_fallback_only_when_nothing_else_matched() {
    // An instrument word wins as usual.
    let r = run_with(&classifier(), "Kits/Kit/cold [140 BPM C minor] piano.wav", Some(&long()));
    assert_eq!(r.category_path, "Melodic Loops/Piano Loops");
    // No key: no guess.
    let r = run_with(&classifier(), "Kits/Kit/SINGLE 145 BPM.wav", Some(&long()));
    assert_eq!(r.category_path, "Uncategorised");
    // A pitched one-shot with no keyword is not a melody loop.
    let short = AudioMeta { duration_secs: Some(1.0), ..Default::default() };
    let r = run_with(&classifier(), "Kits/Kit/glass (c4).wav", Some(&short));
    assert_ne!(r.category_path, "Melodic Loops/Full Melody Loops");
}

#[test]
fn producer_credits_never_match_keywords() {
    // "snare" in a producer handle must not classify the file.
    let r = run("Kits/Kit/sunset [prod. snaremaster] @kickboy.wav");
    assert_eq!(r.category_path, "Uncategorised", "{:#?}", r.evidence);
}

#[test]
fn vibes_is_a_mood_not_a_vibraphone() {
    assert_ne!(run("Kits/Pack/BASSPOP MK1 - BUS VIBES.wav").category_path, "One-Shots/Mallets");
}

#[test]
fn melody_fallback_respects_real_folder_hints() {
    let r = run_with(&classifier(), "Kits/Kit/Vocals/angel [140 BPM C minor].wav", Some(&long()));
    assert_eq!(r.category_path, "Vocals/Vocal Loops", "{:#?}", r.evidence);
    let r = run_with(&classifier(), "Melody Kit Vol 2/angel [140 BPM C minor].wav", Some(&long()));
    assert_eq!(r.category_path, "Melodic Loops/Full Melody Loops", "{:#?}", r.evidence);
}
