# Kit Organizer

**Intelligent music sample organiser for producers.**

Kit Organizer takes sample packs and drum kits from any vendor, whatever folder structure or naming convention they use, and sorts them into one consistent, customisable library. It runs fully offline: no account, no internet, no external APIs.

It is built for electronic producers working in dubstep, riddim, tearout, drum & bass, trap, house, techno and similar genres, so the default dictionary includes terms like growls, yoinks, reeses, wubs, donks and screeches alongside the usual drum and melodic categories.

```
XLNT Riddim Vol 1/DRUMS/ONE SHOTS/KICKS/XLNT_Kick_01.wav  →  Organised Samples/Kicks/
XLNT Riddim Vol 1/Bass/XLNT_Bass_Loop_150BPM.wav          →  Organised Samples/Bass/Bass Loops/
Some Pack/FX/Reverse Crash.wav                            →  Organised Samples/Cymbals/Reverse Cymbals/
Some Pack/Melodic/Piano_Melody_140BPM.wav                 →  Organised Samples/Melodic Loops/Piano Loops/
```

---

## Features

- **Drag and drop import.** Add one or many pack folders, scan them recursively (any depth) and cancel at any time.
- **Rule-based classifier** that uses folder context, filenames, embedded tags, loop/one-shot detection and (optionally) simple audio analysis. Every result comes with a confidence score, the evidence behind it and alternative categories.
- **Learns from your corrections.** Reassign `XLNT_Shot_03.wav` once and every file named like it, in this import or future ones, follows. Kit Organizer also offers to fix the matching files immediately.
- **Keeps construction kits together.** A kit folder (a "Kit" folder with a tempo, like `VFH3 128BPM Shakin Kit`, or any folder inside "Construction Kits"/"Song Kits") goes to `Construction Kits/<kit name>/` with all its parts and sub-folders. Drum kits are not treated as construction kits. To split kits by instrument instead, delete the Construction Kits category.
- **Uses the other files in each folder.** A vaguely named file among confidently classified kicks is treated as a kick.
- **Tolerates messy names:** typos (`Snere`), run-together words (`growlbass`), vendor prefixes (`XLNT_`, `BOS_TT_`) and context-dependent words (`roll` counts as a drum fill only next to a drum word).
- **Preview before anything changes.** Every proposed destination appears in a searchable, sortable review table. Files below your confidence threshold are flagged for review.
- **Manual corrections.** Reassign files one at a time or in bulk, accept predictions, fix the source-pack name, and filter by confidence, category or duplicate status.
- **Safe copy (default) or move.** Copies are verified against a hash before they get their final name. Files are never overwritten (`Kick 01 (2).wav`). Move mode needs explicit confirmation, and an original is only removed after its copy is verified.
- **Duplicate detection** with BLAKE3 content hashes, both within an import and against your existing library. You choose to skip, keep, or be asked. Files that share a name but have different content are never treated as duplicates.
- **Audio preview** for WAV, MP3, AIFF, FLAC and OGG, with a waveform, seeking and looping. Shows duration, sample rate, channels and bit depth. BPM and key appear only when stated in the filename, embedded tags or WAV metadata (ACID tempo, sampler root note), always with their source; otherwise they show as *Unknown*.
- **Library browser** with a folder tree, search and preview.
- **History with safe undo.** Undo deletes only files that are still unmodified, and restores moved files only where the original location is free.
- **Crash recovery.** Every file operation is written to a journal first, so an interrupted run is reconciled on the next launch.
- **Category Manager.** Add, rename, nest, reorder, merge and delete categories. Edit aliases, priorities, loop routing and scoring weights, test filenames live, and import or export JSON configurations.

## Supported files

| Type    | Extensions | Destination |
|---------|------------|-------------|
| Audio   | `.wav .mp3 .aif .aiff .flac .ogg` | classified |
| MIDI    | `.mid .midi` | `MIDI/` |
| Presets | `.fxp .fxb .vstpreset .aupreset .vital .serumpreset .phaseplant .h2p .nmsv .fst .adv .adg .spf .pgtx` | `Presets/` |

Other files are counted and listed but never copied, moved or deleted. OS clutter (`.DS_Store`, `Thumbs.db`, macOS `._` resource forks) is ignored. Kontakt `.nki` files and similar formats are deliberately left out, because they reference sample folders by relative path and would break if moved.

---

## Installation and development

### Prerequisites (all platforms)

- **Node.js 20+** and npm
- **Rust** (stable, 1.87+) via [rustup](https://rustup.rs)

### Windows

1. Install the **Microsoft C++ Build Tools** (Visual Studio Build Tools, workload "Desktop development with C++").
2. **WebView2** comes with Windows 10 (since 1803) and Windows 11. On older systems, install the Evergreen runtime from Microsoft.
3. Install Rust with `rustup-init.exe`, using the default MSVC toolchain.

### macOS

1. Install the Xcode Command Line Tools: `xcode-select --install`
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Linux (Debian/Ubuntu)

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev build-essential
```

### Run in development

```bash
npm install
npm run tauri dev
```

This starts Vite on port 1420 and opens the desktop window with hot reload.

### Production build

```bash
npm run tauri build
```

Installers are written to `target/release/bundle/`:

- **Windows:** `.msi` and NSIS `.exe`
- **macOS:** `.app` and `.dmg` (to build both architectures, add the targets with `rustup target add aarch64-apple-darwin x86_64-apple-darwin` and run `npm run tauri build -- --target universal-apple-darwin`)
- **Linux:** `.deb`, `.rpm` and `.AppImage`

Unsigned builds work locally. To distribute them, set up code signing (Authenticode on Windows, Developer ID and notarisation on macOS) as described in the Tauri distribution guide.

### Tests

```bash
cargo test --workspace   # classifier, scanner, organiser, undo, recovery, audio, DB, accuracy benchmark
npm test                 # UI workflow and helper tests (Vitest + Testing Library)
npm run typecheck
```

### Accuracy benchmark

`crates/kit-core/benchmarks/corpus.txt` holds 252 labelled sample paths in many vendor naming styles (XLNT, Kompany, Black Octopus, trap kits, house, techno, DnB, messy real-world names, ambiguous cases). Run it with:

```bash
cargo run -p kit-core --example benchmark
```

The report shows overall accuracy, how many correct answers were flagged for review, **silent mistakes** (wrong answers above the confidence threshold, the worst kind), recall per category, the most common confusions and every miss. The test suite fails if accuracy drops below 96% or any silent mistake appears.

| Version | Accuracy | Silent mistakes |
|---|---|---|
| Initial rules | 93.7% | 1 |
| + typo/compound/vendor/context matching, sibling context | 96.8% | 0 |

The remaining misses are genuinely ambiguous names (e.g. `Organ_Bass_Hit`, `House_Chord_Stab`) and are all flagged for review. This corpus contains names only, so the gains from learned rules, embedded tags and audio analysis are covered by their own tests rather than by this number.

**Test it on your own library.** Export your file list, label the lines with the category you'd expect, and run the benchmark on it. This is the most useful way to find what to improve.

```powershell
# Windows PowerShell: list every audio file under D:\Samples, relative to it
Get-ChildItem "D:\Samples" -Recurse -Include *.wav,*.mp3,*.aif,*.aiff,*.flac,*.ogg |
  ForEach-Object { "? | " + $_.FullName.Substring("D:\Samples\".Length) } > my-library.txt
```

Replace each `?` with the expected category path, e.g. `Bass/Growls | My Pack/Bass/Growl_01.wav`. Then run:

```bash
cargo run -p kit-core --example benchmark -- my-library.txt
# include your saved categories and learned rules:
cargo run -p kit-core --example benchmark -- my-library.txt --db "%APPDATA%/app.kitorganizer.desktop/kit-organizer.db"
```

None of the tests need commercial sample packs. Filesystem tests generate their files in temporary directories, and the audio tests synthesise WAVs.

### Command-line dry run

To see what the classifier would do with a folder, without changing anything:

```bash
cargo run -p kit-core --example dry_run -- "/path/to/Sample Pack"
```

This prints confidence, sample type and proposed destination for every file. Add `--db <path>` to save the results as an import you can open in the app.

---

## Using Kit Organizer

1. **Import Samples.** Drag pack folders onto the window or click *Browse folders*. Choose an output folder; the library is created inside it as `Organised Samples/`. Pick *Copy* (the default, which leaves your packs untouched) or *Move*, then click **Scan**.
2. **Review.** Every file appears with its predicted category, confidence and destination.
   - Use *Needs review* to see uncertain files. Click a row to hear it and see the evidence behind the classification.
   - Select several rows (Ctrl/⌘-click or Shift-click) to reassign, accept or set the source pack in bulk.
   - *Duplicates* lists files whose content already exists elsewhere. Choose *Skip* or *Keep both*.
   - **Large libraries:** *Needs review* opens in **Groups** view. Uncertain files are grouped by folder and prediction, with the reason they're uncertain (no keywords, only the pack name, conflicting keywords, …). Accept or move a whole group in one click; *Accept all close calls* approves every prediction just under the threshold. Each decision is learned, and the rest of the import is re-classified straight away, so similar files drop out of the queue. In the file list, select a page and click *Select all N matching* to act on every file that matches the current filters.
   - **Export list for tuning** saves the review queue (file names only, no audio) in benchmark format. Correct it and run the benchmark on it, or share it to get the rules tuned for your library.
3. **Organise Samples.** The plan shows how many files will be copied or moved, the total size against free space, which files will be renamed, rerouted or skipped, and sample destinations. Nothing touches the disk until you confirm. Move mode needs an extra confirmation checkbox.
4. **Done.** The completion screen shows counts and any problems. Open the **Library** to browse the result, or undo the import from the completion screen or from **History**.

Unreviewed files below the confidence threshold go to `Uncategorised/` by default. You can change this in Settings to *Use prediction* or *Leave for later*.

---

## Architecture

```
crates/kit-core/          Pure Rust engine (no UI dependencies, fully unit-tested)
  src/classifier/         Tokeniser, BPM/key extraction, loop detection, scoring engine
  src/config/             Category model + full default hierarchy & keyword dictionary
  src/scanner.rs          Recursive, cancellable walker (no symlink following, skips output)
  src/pipeline.rs         Parallel probe + hash + classify; duplicate detection; provenance
  src/organiser/          Plan → journaled execution → recovery → undo
  src/audio/              Header-only metadata (Symphonia + RIFF acid/smpl), preview decoding
  src/db.rs               SQLite (settings, categories, imports, samples, operation journal)
src-tauri/                Tauri 2 shell: IPC commands, background jobs, progress events,
                          kitaudio:// preview protocol, least-privilege capabilities
src/                      React + TypeScript + Tailwind UI
  pages/                  Dashboard, Import, Review, Library, History, Categories, Settings
  components/             layout, ui primitives, import, classification, audio, library, settings
  hooks/ services/ types/ utils/
```

### Key decisions

- **Engine in a separate crate.** All classification and filesystem logic lives in `kit-core`, which does not depend on Tauri. You can test it with plain `cargo test` and reuse it from a CLI. The UI never touches the filesystem directly.
- **Least-privilege webview.** The capability file grants only core APIs and native dialogs. There is no filesystem plugin; every path goes through validated Rust commands. The preview protocol only serves files recorded in the database.
- **Background work, never blocking.** Scanning, analysis and organising run on worker threads and report progress through events. Hashing and header probing use a bounded Rayon pool (2–8 threads by default, configurable). Audio is never fully decoded during a scan, and files are hashed as a stream, so memory stays flat. Review and Library tables page results out of SQLite (200 rows at a time), so tens of thousands of samples stay responsive.
- **Stable category IDs.** Rules refer to categories by ID, not path, so renaming a category never breaks loop routing or default subcategories.
- **Previews are transcoded to WAV in Rust.** Webviews differ in codec support (WebKit has no OGG, Chromium has no AIFF). Decoding with Symphonia and serving 16-bit WAV makes playback identical on every platform.

### How classification works

Classification runs in stages. Each stage adds evidence, and the scores are combined deterministically.

1. **File type.** MIDI and preset files are placed by extension with certainty. Only audio continues to the next stages.
2. **Tokenising.** Names are split on separators, camelCase and letter/digit boundaries (`ClosedHat-03` → `closed hat 03`, `128bpm` → `128 bpm`). Matching is always whole-token, so `bd` never matches inside `Abduction`.
3. **Phrase matching.** The longest phrase wins: `reverse crash` (→ Cymbals/Reverse Cymbals) beats `reverse` + `crash`. Each window is also tried concatenated (`hi hat` = `hihat`) and in singular form (`kicks` → `kick`). Aliases of two characters or fewer, and aliases marked `~`, are *weak* and count half.
4. **Smarter matching.** Leading vendor prefixes (`XLNT_`, `BOS_TT_`, or any unknown ALL-CAPS word that isn't a keyword) are ignored. Run-together words are split into known keywords (`growlbass` → `growl bass`). Aliases can require context: `roll@snare|drum` matches only when a drum word also appears in the filename, folders or tags. If nothing in the filename matches exactly, a misspelt word may match the closest keyword (one edit for 5–7 letters, two for 8+, same first letter) at ×0.6.
5. **Evidence sources.** Filename matches score 3.0, plus 0.5 per extra word. Folder matches score 2.0 for the parent folder, decaying ×0.75 per level up. The imported pack folder counts ×0.4, and broad parent categories count ×0.6. Generic folders (`Samples`, `One Shots`, `Misc`, …) are ignored. Further sources:
   - **Embedded tags** (WAV INFO, MP3 ID3, FLAC/OGG Vorbis comments): title, genre and comment keywords score 1.5. A title that just repeats the filename is ignored.
   - **Learned rules:** a file matching the name pattern of one of your corrections scores 4.0 in the same vendor or pack, 2.4 elsewhere, and half that for partial matches.
   - **Siblings:** in folders of 3+ files where at least two confident files (≥70%, or your manual choices) agree and make up 60%+ of the confident ones, every low-confidence file gets 2.5 × that share.
   - **Audio analysis** (optional, on by default): up to 10 seconds of each file are decoded to measure low-end energy, brightness (zero-crossing rate), attack, decay, number of hits, rhythmic regularity and level trend. These measurements suggest kicks, sustained bass, hi-hats, cymbals, risers and drum loops (about 1.0–1.2 points). Audio evidence is used **only when it agrees with the names, or when there is nothing else to go on**, so it can never override a clearly named file. This is simple signal measurement, not sound recognition: it cannot tell a snare from a clap.
6. **Loop vs one-shot.** Indicators add up: `loop` in the name (+3), a loops folder (+2), a stated BPM (+1.5), an ACID loop tag (+2), long duration (+0.5), one-shot terms (−3), short duration (−1), note + octave like `C4` (−0.5), loop/one-shot words in tags (±1.5), a BPM tag (+1), and from audio analysis a regular rhythm (+1.5) or a single decaying hit (−1). The sample is a loop at a score of 2.0 or more. A BPM tag or a long duration alone is never enough. For loops, evidence is routed through each category's *loop target* (Kicks → Drum Loops/Kick Loops) before scoring, so loop folders support instrument loops instead of competing with them.
7. **Scoring.** For each category, `total = sum over evidence types of the best item of that type + 0.5 × ancestor evidence + 0.25 × priority`. When several unrelated categories appear in a filename, the last strong keyword gets a +0.3 *head-noun* bonus (*Neuro Growl* → Growls, with Neuros listed as an alternative). If the best total is below 0.75, the evidence is too weak and the file goes to Uncategorised. Parent categories with no subcategory evidence go to their default subcategory (Bass → Bass/Other Basses).
8. **Confidence.** `strength = 1 − e^(−S1/2)`, `margin = 1 − 0.5 × S2/S1` (S2 is the best *conflicting* candidate), `confidence = strength × margin`, then ×0.9 when the loop/one-shot decision was borderline and changed the destination. This measures how strong and consistent the rule evidence is. It is **not** a calibrated probability.

Every weight can be changed under **Categories → Rules**.

### Adding AI audio classification later

The classifier collects evidence from a list of `EvidenceSource` implementations (`crates/kit-core/src/classifier/mod.rs`). An audio model only needs to implement that trait and return scored evidence for category IDs with `Origin::Model`. It then joins the existing scoring, confidence and evidence trail without any changes to the UI or the organiser. The current version analyses names, folders, tags and a few simple signal measurements. It does not claim to recognise sounds.

### Data locations

The SQLite database (`kit-organizer.db`) lives in the OS app-data folder:

- **Windows:** `%APPDATA%\app.kitorganizer.desktop\`
- **macOS:** `~/Library/Application Support/app.kitorganizer.desktop/`
- **Linux:** `~/.local/share/app.kitorganizer.desktop/`

Deleting it resets the app. It never deletes any samples.

## License

MIT
