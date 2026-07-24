use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;

use crate::candidate;

use super::{BackendOutcome, ProgressReporter, SEARCH_CHUNK_SIZE, SearchConfig, SearchHit};

pub fn search(config: &SearchConfig, progress: &ProgressReporter) -> BackendOutcome {
    let chunk_count = config.max_index.div_ceil(SEARCH_CHUNK_SIZE);
    let processed = AtomicU64::new(0);
    let stopped = AtomicBool::new(false);

    let hit = (0..chunk_count).into_par_iter().find_map_any(|chunk| {
        if stopped.load(Ordering::Relaxed) {
            return None;
        }

        let start = chunk * SEARCH_CHUNK_SIZE;
        let end = (start + SEARCH_CHUNK_SIZE).min(config.max_index);
        let mut local_processed = 0u64;

        for index in start..end {
            if stopped.load(Ordering::Relaxed) {
                break;
            }

            local_processed += 1;
            let hash = candidate::digest(index, config.candidate);
            if candidate::matches_leading_zero_bits(
                candidate::digest_word(&hash),
                config.leading_zero_bits,
            ) {
                if !stopped.swap(true, Ordering::Relaxed) {
                    processed.fetch_add(local_processed, Ordering::Relaxed);
                    progress(local_processed);
                    return Some(SearchHit { index });
                }
                break;
            }
        }

        processed.fetch_add(local_processed, Ordering::Relaxed);
        progress(local_processed);
        None
    });

    BackendOutcome {
        hit,
        processed: processed.load(Ordering::Relaxed),
        backend_name: "scalar".to_owned(),
        device_name: None,
        fallback_reasons: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn bounded_search_reports_a_valid_hit() {
        let progress = Arc::new(Mutex::new(0u64));
        let progress_callback: ProgressReporter = {
            let progress = Arc::clone(&progress);
            Arc::new(move |count| *progress.lock().expect("progress lock") += count)
        };
        let config = SearchConfig {
            candidate: crate::candidate::CandidateConfig {
                commit: 0xeb366895,
                node_suffix: 0xebac62b9,
            },
            max_index: 256,
            leading_zero_bits: 0,
        };

        let outcome = search(&config, &progress_callback);
        let hit = outcome.hit.expect("zero-bit predicate must hit");
        assert!(hit.index < config.max_index);
        assert_eq!(outcome.processed, 1);
    }
}
