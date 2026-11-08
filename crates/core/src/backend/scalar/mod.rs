use rayon::prelude::*;

use crate::candidate;

use super::{BackendOutcome, ProgressReporter, SEARCH_CHUNK_SIZE, SearchConfig};

pub fn search(config: &SearchConfig, progress: &ProgressReporter) -> BackendOutcome {
    let mut start = config.start_index;
    let mut evaluated = 0_u64;
    let hit = 'search: {
        while start < config.max_index {
            let end = (start + SEARCH_CHUNK_SIZE).min(config.max_index);
            evaluated += end - start;
            let min_index = (start..end)
                .into_par_iter()
                .filter_map(|index| {
                    candidate::matches_leading_zero_bits(
                        candidate::digest_word(&candidate::digest(index, config.candidate)),
                        config.leading_zero_bits,
                    )
                    .then_some(index)
                })
                .min();
            if let Some(index) = min_index {
                progress(index + 1 - start);
                break 'search Some(index);
            }
            progress(end - start);
            start = end;
        }
        None
    };

    BackendOutcome {
        hit,
        processed: hit.map_or(config.max_index, |index| index + 1),
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
