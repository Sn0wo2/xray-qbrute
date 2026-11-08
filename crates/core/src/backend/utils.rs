use crate::candidate::{self, CandidateConfig};

use super::SearchError;

pub(crate) const SELF_TEST_BASES: [u64; 4] = [0, 1 << 28, 1 << 30, (1 << 58) - 256];
pub(crate) const SELF_TEST_COUNT: u64 = 256;

pub(crate) fn verify_digest_words(
    words: &[u64],
    base: u64,
    config: CandidateConfig,
    backend: &str,
) -> Result<(), SearchError> {
    for (offset, &actual) in words.iter().enumerate() {
        let index = base + offset as u64;
        let expected = candidate::digest_word(&candidate::digest(index, config));
        if actual != expected {
            return Err(SearchError::Runtime(format!(
                "{backend} SHA-512 self-test failed at index {index}: \
                 produced 0x{actual:016x}, expected 0x{expected:016x}"
            )));
        }
    }
    Ok(())
}
