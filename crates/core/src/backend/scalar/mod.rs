use rayon::prelude::*;

use crate::candidate;

use super::{BackendOutcome, ProgressReporter, SEARCH_CHUNK_SIZE, SearchConfig, SearchHit};

pub fn search(config: &SearchConfig, progress: &ProgressReporter) -> BackendOutcome {
    let mut start = config.start_index;
    let mut evaluated = 0_u64;

    while start < config.max_index {
        let end = (start + SEARCH_CHUNK_SIZE).min(config.max_index);
        let chunk_size = end - start;

        let min_index = (start..end)
            .into_par_iter()
            .filter_map(|i| {
                let hash = candidate::digest(i, config.candidate);
                candidate::matches_leading_zero_bits(
                    candidate::digest_word(&hash),
                    config.leading_zero_bits,
                )
                .then_some(i)
            })
            .min();

        evaluated += chunk_size;

        if let Some(index) = min_index {
            let processed = index + 1;
            progress(processed - start);
            return BackendOutcome {
                hit: Some(SearchHit { index }),
                processed,
                evaluated,
                backend_name: "scalar".to_owned(),
                device_name: None,
                fallback_reasons: Vec::new(),
                search_elapsed: None,
                kernel_config: None,
                tuning_elapsed: None,
            };
        }

        progress(chunk_size);
        start = end;
    }

    BackendOutcome {
        hit: None,
        processed: config.max_index,
        evaluated,
        backend_name: "scalar".to_owned(),
        device_name: None,
        fallback_reasons: Vec::new(),
        search_elapsed: None,
        kernel_config: None,
        tuning_elapsed: None,
    }
}

#[cfg(test)]
mod test;
