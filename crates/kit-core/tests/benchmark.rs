//! Regression guard: classification accuracy on the bundled corpus must not
//! drop. Run `cargo run -p kit-core --example benchmark` for the full report.

use kit_core::benchmark;
use kit_core::classifier::Classifier;
use kit_core::config::{default_config, CompiledConfig};

/// Raise this when accuracy improves; never lower it to make a change pass.
const MIN_ACCURACY: f32 = 0.96;

#[test]
fn corpus_accuracy() {
    let cases = benchmark::parse(include_str!("../benchmarks/corpus.txt")).unwrap();
    let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
    let report = benchmark::run(&c, &cases, 0.6);
    println!("{report}");
    assert!(report.unknown_expected.is_empty(), "corpus refers to unknown categories: {:?}", report.unknown_expected);
    assert!(report.accuracy() >= MIN_ACCURACY, "accuracy fell below {MIN_ACCURACY}:\n{report}");
    // Wrong answers above the review threshold are never acceptable.
    assert_eq!(report.wrong_high_confidence, 0, "confident mistakes:\n{report}");
}
