//! The default category hierarchy and keyword dictionary.
//!
//! Written as a small builder DSL so the dictionary stays readable. Ids are
//! derived from slugged paths, so `"Drum Loops/Kick Loops"` has the id
//! `drum-loops/kick-loops`.

use super::{CategoryConfig, CategoryNode, ClassifierWeights, KindHint};

struct B {
    name: &'static str,
    aliases: &'static [&'static str],
    kind: KindHint,
    loop_to: Option<&'static str>,
    default_child: Option<&'static str>,
    priority: i32,
    kids: Vec<B>,
}

fn c(name: &'static str, aliases: &'static [&'static str]) -> B {
    B { name, aliases, kind: KindHint::Any, loop_to: None, default_child: None, priority: 0, kids: vec![] }
}

impl B {
    fn loop_to(mut self, path: &'static str) -> Self {
        self.loop_to = Some(path);
        self
    }
    fn default_child(mut self, name: &'static str) -> Self {
        self.default_child = Some(name);
        self
    }
    fn kind(mut self, k: KindHint) -> Self {
        self.kind = k;
        self
    }
    fn priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    fn kids(mut self, kids: Vec<B>) -> Self {
        self.kids = kids;
        self
    }
}

pub fn slug(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            let mut s = String::new();
            for ch in seg.chars() {
                if ch.is_alphanumeric() {
                    s.extend(ch.to_lowercase());
                } else if !s.ends_with('-') && !s.is_empty() {
                    s.push('-');
                }
            }
            s.trim_end_matches('-').to_string()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn build(b: B, prefix: &str) -> CategoryNode {
    let path = if prefix.is_empty() { b.name.to_string() } else { format!("{prefix}/{}", b.name) };
    // Kinds propagate to children of loop containers.
    let kids = b
        .kids
        .into_iter()
        .map(|mut k| {
            if b.kind == KindHint::Loop && k.kind == KindHint::Any {
                k.kind = KindHint::Loop;
            }
            build(k, &path)
        })
        .collect();
    CategoryNode {
        id: slug(&path),
        name: b.name.to_string(),
        aliases: b.aliases.iter().map(|s| s.to_string()).collect(),
        priority: b.priority,
        kind: b.kind,
        loop_target: b.loop_to.map(slug),
        default_child: b.default_child.map(|n| slug(&format!("{path}/{n}"))),
        children: kids,
    }
}

const BASS_LOOPS: &str = "Bass/Bass Loops";
const PERC_LOOPS: &str = "Drum Loops/Percussion Loops";

#[rustfmt::skip]
fn tree() -> Vec<B> {
    use KindHint::*;
    vec![
        c("Bass", &["bass", "basses", "bass one shot"]).default_child("Other Basses").loop_to(BASS_LOOPS).kids(vec![
            c("808s", &["808", "808s", "808 bass", "tr 808 bass", "808 sub"]).loop_to(BASS_LOOPS),
            c("Subs", &["sub", "subs", "sub bass", "subbass", "sine bass", "~sine"]).loop_to(BASS_LOOPS),
            c("Growls", &["growl", "growls", "growler", "growly"]).loop_to(BASS_LOOPS),
            c("Screeches", &["screech", "screeches", "screecher", "screetch", "screechy"]).loop_to(BASS_LOOPS),
            c("Sustains", &["sustain", "sustains", "sustained", "sus bass", "sustain bass"]).loop_to(BASS_LOOPS),
            c("Reeses", &["reese", "reeses", "reece", "reese bass"]).loop_to(BASS_LOOPS),
            c("Neuros", &["neuro", "neuros", "neuro bass", "neurofunk bass"]).loop_to(BASS_LOOPS),
            c("Wubs", &["wub", "wubs", "wub bass"]).loop_to(BASS_LOOPS),
            c("Wobbles", &["wobble", "wobbles", "wob", "wobs", "wobble bass"]).loop_to(BASS_LOOPS),
            c("Yoinks", &["yoink", "yoinks"]).loop_to(BASS_LOOPS),
            c("Gunshots", &["gunshot", "gunshots", "gun shot", "gunshot bass", "gun shot bass"]).loop_to(BASS_LOOPS),
            c("Donks", &["donk", "donks", "donk bass"]).loop_to(BASS_LOOPS),
            c("FM Basses", &["fm bass", "fm basses", "~fm", "fm growl"]).loop_to(BASS_LOOPS),
            c("Riddim Basses", &["riddim bass", "riddim basses", "~riddim"]).loop_to(BASS_LOOPS),
            c("Bass Stabs", &["bass stab", "bass stabs", "bass hit", "bass hits", "bass shot"]).loop_to(BASS_LOOPS).priority(1),
            c("Glitch Basses", &["glitch bass", "glitch basses", "glitchy bass"]).loop_to(BASS_LOOPS),
            c("Metallic Basses", &["metallic bass", "metal bass", "metallic", "metalic bass"]).loop_to(BASS_LOOPS),
            c("Bass Guitars", &["bass guitar", "bass guitars", "electric bass", "slap bass", "fingered bass", "picked bass", "upright bass", "double bass"]).loop_to(BASS_LOOPS),
            c("Bass Loops", &["bass loop", "bass loops", "bassline", "basslines", "bass line", "bass lines"]).kind(Loop),
            c("Other Basses", &["other bass", "misc bass", "mid bass", "midbass", "~mids"]),
        ]),
        c("Kicks", &["kick", "kicks", "kik", "kck", "bd", "bassdrum", "bass drum", "kick drum", "808 kick", "909 kick", "kick one shot"]).loop_to("Drum Loops/Kick Loops"),
        c("Snares", &["snare", "snares", "snr", "sd", "snare drum", "808 snare", "909 snare"]).loop_to("Drum Loops/Snare Loops"),
        c("Claps", &["clap", "claps", "clp", "handclap", "handclaps", "hand clap", "808 clap", "909 clap"]).loop_to("Drum Loops/Clap Loops"),
        c("Rims", &["rim", "rims", "rimshot", "rimshots", "rim shot", "sidestick", "side stick"]).loop_to(PERC_LOOPS),
        c("Snaps", &["snap", "snaps", "finger snap", "fingersnap", "finger snaps"]).loop_to(PERC_LOOPS),
        c("Hi-Hats", &["hihat", "hi hat", "hihats", "hi hats", "hat", "hats", "hh", "hhat", "808 hat", "808 hihat"]).loop_to("Drum Loops/Hi-Hat Loops").kids(vec![
            c("Closed Hats", &["closed hat", "closedhat", "closed hats", "closed hihat", "closed hi hat", "closed hh", "chh", "~ch", "cl hat", "clhat", "~closed"]).loop_to("Drum Loops/Hi-Hat Loops"),
            c("Open Hats", &["open hat", "openhat", "open hats", "open hihat", "open hi hat", "open hh", "ohh", "~oh", "op hat", "ophat", "~open"]).loop_to("Drum Loops/Hi-Hat Loops"),
            c("Pedal Hats", &["pedal hat", "pedal hihat", "pedal hi hat", "pedal hh", "phh", "foot hat", "~pedal"]).loop_to("Drum Loops/Hi-Hat Loops"),
        ]),
        c("Cymbals", &["cymbal", "cymbals", "cym", "cyms"]).default_child("Other Cymbals").loop_to("Drum Loops/Cymbal Loops").kids(vec![
            c("Crashes", &["crash", "crashes", "crsh", "crash cymbal"]).loop_to("Drum Loops/Cymbal Loops"),
            c("Rides", &["ride", "rides", "ride bell", "ride cymbal"]).loop_to("Drum Loops/Cymbal Loops"),
            c("Splashes", &["splash", "splashes", "splash cymbal"]).loop_to("Drum Loops/Cymbal Loops"),
            c("Chinas", &["china", "chinas", "china cymbal"]).loop_to("Drum Loops/Cymbal Loops"),
            c("Reverse Cymbals", &["reverse crash", "reversed crash", "rev crash", "crash reverse", "reverse cymbal", "reversed cymbal", "rev cymbal", "reverse cym", "rev cym", "reverse ride", "reverse splash", "reverse china", "crash rev"]),
            c("Other Cymbals", &["other cymbal", "misc cymbal", "gong", "gongs"]),
        ]),
        c("Percussion", &["perc", "percs", "percussion", "percussions", "perc hit"]).default_child("Other Percussion").loop_to(PERC_LOOPS).kids(vec![
            c("Shakers", &["shaker", "shakers", "shkr"]).loop_to(PERC_LOOPS),
            c("Tambourines", &["tambourine", "tambourines", "tamb", "tambo", "tamborine", "tambourin"]).loop_to(PERC_LOOPS),
            c("Congas", &["conga", "congas"]).loop_to(PERC_LOOPS),
            c("Bongos", &["bongo", "bongos"]).loop_to(PERC_LOOPS),
            c("Cowbells", &["cowbell", "cowbells", "cow bell", "808 cowbell"]).loop_to(PERC_LOOPS),
            c("Woodblocks", &["woodblock", "woodblocks", "wood block", "wood blocks"]).loop_to(PERC_LOOPS),
            c("Claves", &["clave", "claves"]).loop_to(PERC_LOOPS),
            c("Maracas", &["maraca", "maracas"]).loop_to(PERC_LOOPS),
            c("Guiros", &["guiro", "guiros"]).loop_to(PERC_LOOPS),
            c("Agogos", &["agogo", "agogos"]).loop_to(PERC_LOOPS),
            c("Timpani", &["timpani", "timpanis", "timp", "kettle drum", "kettledrum"]).loop_to(PERC_LOOPS),
            c("Hand Drums", &["hand drum", "hand drums", "djembe", "tabla", "darbuka", "cajon", "frame drum", "taiko", "doumbek"]).loop_to(PERC_LOOPS),
            c("Foley Percussion", &["foley perc", "foley percussion", "foley hit", "foley percs"]).loop_to(PERC_LOOPS),
            c("Electronic Percussion", &["electronic perc", "electronic percussion", "electro perc", "synth perc", "zap", "zaps", "blip", "blips"]).loop_to(PERC_LOOPS),
            c("Other Percussion", &["triangle", "cabasa", "vibraslap", "other perc", "misc perc"]).loop_to(PERC_LOOPS),
        ]),
        c("Toms", &["tom", "toms", "floor tom", "rack tom", "hi tom", "low tom", "mid tom", "tom tom"]).loop_to("Drum Loops/Tom Loops"),
        c("FX", &["fx", "sfx", "efx", "effect", "effects", "fx shot"]).default_child("Other FX").kids(vec![
            c("Risers", &["riser", "risers", "uplifter", "uplifters", "up lifter", "rise", "build up", "buildup", "upsweep", "riser fx"]),
            c("Downlifters", &["downlifter", "downlifters", "down lifter", "faller", "fallers", "downsweep", "down sweep", "fall"]),
            c("Impacts", &["impact", "impacts", "hit fx", "boom", "booms", "sub drop", "subdrop", "sub drops", "slam"]),
            c("Sweeps", &["sweep", "sweeps", "noise sweep", "white noise sweep", "filter sweep"]),
            c("Transitions", &["transition", "transitions", "swell", "swells", "whoosh", "whooshes", "woosh", "swoosh"]),
            c("Reverse FX", &["reverse", "reversed", "rev", "reverses", "reverse fx", "reversed fx"]),
            c("Glitches", &["glitch", "glitches", "glitch fx", "glitchy"]),
            c("Stutters", &["stutter", "stutters", "stutter fx", "stuttered", "stuttering"]),
            c("Noise FX", &["noise", "noises", "white noise", "noise fx", "static", "crackle", "vinyl crackle", "vinyl noise", "hiss"]),
            c("Atmospheres", &["atmosphere", "atmospheres", "atmos", "atmo", "ambience", "ambiences", "ambient", "ambiance"]),
            c("Drones", &["drone", "drones"]),
            c("Textures", &["texture", "textures"]),
            c("Foley FX", &["foley", "foley fx", "field recording", "field recordings"]),
            c("Cinematic FX", &["cinematic", "cinematic fx", "trailer", "braam", "braams"]),
            c("Other FX", &["other fx", "misc fx", "laser", "lasers", "siren", "sirens", "alarm", "scratch", "scratches", "vinyl scratch", "tape stop", "tapestop"]),
        ]),
        c("Drum Loops", &["drum loop", "drum loops", "drumloop", "drumloops", "drums loop"]).kind(Loop).default_child("Full Loops").kids(vec![
            c("Full Loops", &["full loop", "full loops", "full drum loop", "full drums", "full beat", "~beat", "~beats", "~groove", "~grooves", "drum groove", "drum beat"]),
            c("Top Loops", &["top loop", "top loops", "~top", "~tops", "drum top", "drum tops"]),
            c("Kick Loops", &["kick loop", "kick loops"]),
            c("Snare Loops", &["snare loop", "snare loops"]),
            c("Clap Loops", &["clap loop", "clap loops"]),
            c("Hi-Hat Loops", &["hat loop", "hat loops", "hats loop", "hihat loop", "hi hat loop", "hh loop", "hihat loops"]),
            c("Percussion Loops", &["perc loop", "perc loops", "percussion loop", "percussion loops"]),
            c("Cymbal Loops", &["cymbal loop", "cymbal loops", "ride loop", "crash loop"]),
            c("Tom Loops", &["tom loop", "tom loops"]),
            c("Drum Fills", &["fill", "fills", "drum fill", "drum fills", "tom fill", "snare fill", "snare roll", "snare rolls", "roll", "rolls"]),
            c("Breakbeats", &["break", "breaks", "breakbeat", "breakbeats", "drum break", "drum breaks", "amen"]),
            c("Other Drum Loops", &["other drum loop", "misc drum loop"]),
        ]),
        c("Melodic Loops", &["melodic loop", "melodic loops", "melody loop", "melody loops", "music loop", "music loops", "melodic", "~music"]).kind(Loop).default_child("Other Melodic Loops").kids(vec![
            c("Piano Loops", &["piano loop", "piano loops", "keys loop", "keys loops", "rhodes loop"]),
            c("Guitar Loops", &["guitar loop", "guitar loops", "gtr loop"]),
            c("Synth Loops", &["synth loop", "synth loops"]),
            c("Pluck Loops", &["pluck loop", "pluck loops"]),
            c("Lead Loops", &["lead loop", "lead loops"]),
            c("Pad Loops", &["pad loop", "pad loops"]),
            c("Bell Loops", &["bell loop", "bell loops"]),
            c("Mallet Loops", &["mallet loop", "mallet loops", "marimba loop"]),
            c("String Loops", &["string loop", "string loops", "strings loop"]),
            c("Brass Loops", &["brass loop", "brass loops"]),
            c("Woodwind Loops", &["woodwind loop", "woodwind loops", "flute loop"]),
            c("Arpeggio Loops", &["arp loop", "arp loops", "arpeggio", "arpeggios", "arpeggio loop", "arpeggiated"]),
            c("Chord Loops", &["chord loop", "chord loops", "chords loop", "chord progression", "chord progressions", "progression"]),
            c("Orchestral Loops", &["~orchestral", "~orchestra", "orchestral loop", "orchestral loops"]),
            c("Full Melody Loops", &["~melody", "~melodies", "full melody", "full melody loop", "composition", "song starter", "song starters"]),
            c("Other Melodic Loops", &["other melodic loop"]),
        ]),
        c("One-Shots", &["melodic one shot", "melodic one shots", "music one shot", "instrument one shot"]).default_child("Other One-Shots").loop_to("Melodic Loops/Other Melodic Loops").kids(vec![
            c("Plucks", &["pluck", "plucks", "plk", "plucked"]).loop_to("Melodic Loops/Pluck Loops"),
            c("Leads", &["lead", "leads", "ld", "synth lead"]).loop_to("Melodic Loops/Lead Loops"),
            c("Pads", &["pad", "pads", "synth pad"]).loop_to("Melodic Loops/Pad Loops"),
            c("Keys", &["keys", "keyboard", "keyboards"]).loop_to("Melodic Loops/Piano Loops").kids(vec![
                c("Pianos", &["piano", "pianos", "pno", "grand piano", "upright piano", "felt piano"]).loop_to("Melodic Loops/Piano Loops"),
                c("Electric Pianos", &["electric piano", "electric pianos", "epiano", "e piano", "ep", "rhodes", "wurli", "wurlitzer"]).loop_to("Melodic Loops/Piano Loops"),
                c("Organs", &["organ", "organs", "hammond", "b3 organ"]).loop_to("Melodic Loops/Piano Loops"),
            ]),
            c("Synths", &["synth", "synths", "syn", "synthesizer", "synthesiser"]).loop_to("Melodic Loops/Synth Loops"),
            c("Bells", &["bell", "bells", "chime", "chimes", "glockenspiel", "glock", "celesta", "music box"]).loop_to("Melodic Loops/Bell Loops"),
            c("Mallets", &["mallet", "mallets", "marimba", "xylophone", "vibraphone", "vibes", "kalimba"]).loop_to("Melodic Loops/Mallet Loops"),
            c("Strings", &["string", "strings", "violin", "violins", "viola", "cello", "cellos", "pizzicato", "pizz", "str"]).loop_to("Melodic Loops/String Loops"),
            c("Brass", &["brass", "trumpet", "trumpets", "trombone", "horn", "horns", "tuba", "french horn"]).loop_to("Melodic Loops/Brass Loops"),
            c("Woodwinds", &["woodwind", "woodwinds", "flute", "flutes", "clarinet", "oboe", "bassoon", "sax", "saxophone", "pan flute", "recorder"]).loop_to("Melodic Loops/Woodwind Loops"),
            c("Guitars", &["guitar", "guitars", "gtr", "acoustic guitar", "electric guitar"]).loop_to("Melodic Loops/Guitar Loops"),
            c("Arps", &["arp", "arps"]).loop_to("Melodic Loops/Arpeggio Loops"),
            c("Chords", &["chord", "chords", "chrd", "chord shot"]).loop_to("Melodic Loops/Chord Loops"),
            c("Stabs", &["stab", "stabs", "synth stab", "chord stab", "orch hit", "orchestra hit", "orchestral hit"]).loop_to("Melodic Loops/Synth Loops"),
            c("Textures", &["texture shot", "tonal texture", "texture one shot"]),
            c("Other One-Shots", &["other one shot", "misc one shot"]),
        ]),
        c("Vocals", &["vocal", "vocals", "vox", "voc", "voice", "voices", "vocal sample"]).default_child("Other Vocals").loop_to("Vocals/Vocal Loops").kids(vec![
            c("Vocal Chops", &["vocal chop", "vocal chops", "vox chop", "vox chops", "chop", "chops", "chopped vocal", "chopped"]),
            c("Vocal One-Shots", &["vocal one shot", "vocal one shots", "vocal shot", "vocal shots", "vox shot", "vocal hit", "vox hit"]),
            c("Ad-libs", &["adlib", "adlibs", "ad lib", "ad libs"]),
            c("Chants", &["chant", "chants", "shout", "shouts", "crowd chant"]),
            c("Vocal Phrases", &["phrase", "phrases", "vocal phrase", "vocal phrases", "vox phrase", "hook", "hooks"]),
            c("Acapellas", &["acapella", "acapellas", "a cappella", "acappella", "accapella", "a capella"]),
            c("Vocal Loops", &["vocal loop", "vocal loops", "vox loop", "vox loops"]).kind(Loop),
            c("Harmonies", &["harmony", "harmonies", "vocal harmony"]),
            c("Choirs", &["choir", "choirs", "choral"]),
            c("Spoken Word", &["spoken", "spoken word", "speech", "narration", "dialogue", "dialog"]),
            c("Rap Vocals", &["rap", "raps", "rap vocal", "rap vocals", "verse"]),
            c("Vocal FX", &["vocal fx", "vox fx", "vocal effect", "vocal effects", "vocoder", "talkbox", "vocal riser", "vocal sweep"]),
            c("Other Vocals", &["other vocal", "misc vocal"]),
        ]),
        c("MIDI", &["midi", "midis", "midi file", "midi files"]),
        c("Presets", &["preset", "presets"]),
        c("Uncategorised", &[]),
    ]
}

pub fn default_config() -> CategoryConfig {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    CategoryConfig {
        version: 1,
        root_folder: "Organised Samples".into(),
        categories: tree().into_iter().map(|b| build(b, "")).collect(),
        weights: ClassifierWeights::default(),
        generic_folder_terms: s(&[
            "samples", "sample", "audio", "sounds", "sound", "one shots", "one shot", "oneshots", "oneshot",
            "misc", "miscellaneous", "various", "other", "wav", "wavs", "aiff", "mp3", "flac", "24 bit", "16 bit",
            "content", "files", "kit", "kits", "drum kit", "drum kits", "pack", "sample pack", "library",
            "drums", "drum", "drum shots", "drum one shots", "loops", "loop", "sounds and loops", "stuff",
            "extras", "bonus", "free", "demo", "processed", "dry", "wet", "new folder",
        ]),
        loop_terms: s(&[
            "loop", "loops", "lp", "groove", "grooves", "beat", "beats", "melody", "melodies", "fill", "fills",
            "break", "breaks", "breakbeat", "progression", "bassline", "arpeggio", "acapella", "song starter",
        ]),
        one_shot_terms: s(&[
            "one shot", "one shots", "oneshot", "oneshots", "os", "shot", "shots", "hit", "hits", "single", "stab",
        ]),
        midi_category: "midi".into(),
        preset_category: "presets".into(),
        uncategorised_category: "uncategorised".into(),
    }
}
