//! What-if analysis for an exported review list: re-classifies every file
//! with the current default rules (names only) and reports how the review
//! queue would change, which folders become confident, and what is left.
//!
//! ```text
//! cargo run --release -p kit-core --example review_whatif -- kit-organizer-review-list.txt
//! ```
use kit_core::benchmark::parse;
use kit_core::classifier::Classifier;
use kit_core::config::{default_config, CompiledConfig};
use kit_core::filetypes::FileKind;
use kit_core::pipeline::{classify_batch, BatchItem};
use std::collections::BTreeMap;
fn main() {
    let text = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let cases = parse(&text).unwrap();
    let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
    let split: Vec<(String, Vec<String>, String)> = cases.iter().map(|c| { let mut p: Vec<String> = c.path.split('/').map(String::from).collect(); let f = p.pop().unwrap(); let g = p.join("/"); (f, p, g) }).collect();
    let items: Vec<BatchItem> = split.iter().map(|(f, p, g)| BatchItem { file_name: f, folders: p, kind: FileKind::Audio, audio: None, group: g, fixed: None }).collect();
    let res = classify_batch(&items, &c);
    let (mut unc, mut conf, mut before_unc) = (0, 0, 0);
    let mut cats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (case, r) in cases.iter().zip(&res) {
        let r = r.as_ref().unwrap();
        if case.expected == "Uncategorised" { before_unc += 1; }
        if r.category_path == "Uncategorised" { unc += 1; }
        if r.confidence >= 0.6 { conf += 1; }
        let e = cats.entry(r.category_path.clone()).or_default();
        e.0 += 1; if r.confidence >= 0.6 { e.1 += 1; }
        let ex = examples.entry(r.category_path.clone()).or_default();
        if ex.len() < 4 && case.expected == "Uncategorised" { ex.push(format!("{:.0}% {}", r.confidence * 100.0, case.path.rsplit('/').next().unwrap())); }
    }
    let mut confident_groups: BTreeMap<(String, String), (usize, String)> = BTreeMap::new();
    for (case, r) in cases.iter().zip(&res) {
        let r = r.as_ref().unwrap();
        if case.expected == "Uncategorised" && r.confidence >= 0.6 {
            let folder = case.path.rsplit('/').nth(1).unwrap_or("").to_string();
            let e = confident_groups.entry((folder, r.category_path.clone())).or_insert((0, case.path.rsplit('/').next().unwrap().to_string()));
            e.0 += 1;
        }
    }
    let mut cg: Vec<_> = confident_groups.into_iter().collect();
    cg.sort_by_key(|g| std::cmp::Reverse(g.1 .0));
    let mut left: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (case, r) in cases.iter().zip(&res) {
        if r.as_ref().unwrap().category_path == "Uncategorised" {
            let folder = case.path.rsplit('/').nth(1).unwrap_or("").to_string();
            let e = left.entry(folder).or_insert((0, case.path.rsplit('/').next().unwrap().to_string()));
            e.0 += 1;
        }
    }
    let mut lv: Vec<_> = left.into_iter().collect();
    lv.sort_by_key(|g| std::cmp::Reverse(g.1 .0));
    let mut lost: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (case, r) in cases.iter().zip(&res) {
        let r = r.as_ref().unwrap();
        if case.expected != "Uncategorised" && r.category_path == "Uncategorised" {
            let folder = case.path.rsplit('/').nth(1).unwrap_or("").to_string();
            let reason = r.evidence.first().cloned().unwrap_or_default();
            let short = if reason.contains("DAW") { "DAW project" } else if reason.contains("too weak") { "too weak" } else { "no keywords" };
            *lost.entry((format!("{} <- was {}", short, case.expected), folder)).or_default() += 1;
        }
    }
    let mut lo: Vec<_> = lost.into_iter().collect();
    lo.sort_by_key(|g| std::cmp::Reverse(g.1));
    println!("NEWLY UNCATEGORISED (names only, no audio)");
    for ((r, f), n) in lo.iter().take(25) { println!("{n:5} {r:<52} [{f}]"); }
    println!("STILL UNCATEGORISED");
    for (f, (n, ex)) in lv.iter().take(30) { println!("{n:5} [{f}] e.g. {ex}"); }
    println!("CONFIDENT NEW GROUPS");
    for ((f, c), (n, ex)) in &cg { println!("{n:5} {c:<32} [{f}] e.g. {ex}"); }
    println!("files {}: uncategorised before {before_unc} → after {unc}; now above 60%: {conf}", cases.len());
    let mut v: Vec<_> = cats.into_iter().collect();
    v.sort_by_key(|g| std::cmp::Reverse(g.1 .0));
    for (k, (n, hi)) in v.iter().take(28) { println!("{n:6} ({hi:5} ≥60%)  {k:<34} {}", examples.get(k).map(|e| e.join(" | ")).unwrap_or_default()); }
}
