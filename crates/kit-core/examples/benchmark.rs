//! Measure classification accuracy on labelled sample lists.
//!
//! ```text
//! cargo run -p kit-core --example benchmark                       # built-in corpus
//! cargo run -p kit-core --example benchmark -- my-library.txt      # your own list
//! cargo run -p kit-core --example benchmark -- my.txt --db <path>  # with your saved categories + learned rules
//! ```
//!
//! List format (one per line): `Expected/Category | Pack/Folder/File.wav`.

use kit_core::benchmark;
use kit_core::classifier::Classifier;
use kit_core::config::{default_config, CompiledConfig};
use kit_core::db::Database;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let take = |args: &mut Vec<String>, flag: &str| {
        args.iter().position(|a| a == flag).map(|i| {
            args.remove(i);
            args.remove(i)
        })
    };
    let db = take(&mut args, "--db").map(PathBuf::from);
    let threshold: f32 = take(&mut args, "--threshold").map(|t| t.parse()).transpose()?.unwrap_or(0.6);
    if args.is_empty() {
        args.push(concat!(env!("CARGO_MANIFEST_DIR"), "/benchmarks/corpus.txt").to_string());
    }
    let classifier = match &db {
        Some(p) => {
            let db = Database::open(p)?;
            Classifier::new(CompiledConfig::compile(&db.category_config()?)?).with_learned(db.learned_rules()?)
        }
        None => Classifier::new(CompiledConfig::compile(&default_config())?),
    };
    let mut cases = Vec::new();
    for f in &args {
        cases.extend(benchmark::parse(&std::fs::read_to_string(f)?)?);
    }
    let report = benchmark::run(&classifier, &cases, threshold);
    println!("{report}");
    Ok(())
}
