use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn bounded_search_reports_a_valid_hit() {
    let progress = Arc::new(Mutex::new(0_u64));
    let progress_callback: ProgressReporter = {
        let progress = Arc::clone(&progress);
        Arc::new(move |count| *progress.lock().expect("progress lock") += count)
    };
    let config = SearchConfig {
        candidate: candidate::CandidateConfig {
            commit: 0xeb366895,
            node_suffix: 0xebac62b9,
        },
        start_index: 0,
        max_index: 256,
        leading_zero_bits: 0,
    };

    let outcome = search(&config, &progress_callback);
    let hit = outcome.hit.expect("zero-bit predicate must hit");
    assert!(hit.index < config.max_index);
    assert_eq!(outcome.processed, 1);
}
