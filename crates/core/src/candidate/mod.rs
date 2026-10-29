use sha2::{Digest, Sha512};

pub const MAX_UUID_INDEX: u64 = 1_u64 << 58;
pub const DEFAULT_DIFFICULTY_BITS: u32 = 33;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::exhaustive_structs, clippy::module_name_repetitions)]
pub struct CandidateConfig {
    pub commit: u32,
    pub node_suffix: u32,
}

#[allow(clippy::module_name_repetitions)]
pub fn candidate_words(index: u64, config: CandidateConfig) -> [u64; 2] {
    let node_prefix = index & 0xFFFF;
    let variant_tail = (index >> 16) & 0xFFF;
    let variant_head = (index >> 28) & 0x3;
    let time_high = (index >> 30) & 0xFFF;
    let time_mid = (index >> 42) & 0xFFFF;

    let word0 = (u64::from(config.commit) << 32) | (time_mid << 16) | 0x4000 | time_high;
    let variant = ((8 + variant_head) << 12) | variant_tail;
    let word1 = (variant << 48) | (node_prefix << 32) | u64::from(config.node_suffix);

    [word0, word1]
}

#[allow(clippy::module_name_repetitions)]
pub fn candidate_bytes(index: u64, config: CandidateConfig) -> [u8; 16] {
    let words = candidate_words(index, config);
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&words[0].to_be_bytes());
    bytes[8..].copy_from_slice(&words[1].to_be_bytes());
    bytes
}

pub fn digest(index: u64, config: CandidateConfig) -> [u8; 64] {
    let output = Sha512::digest(candidate_bytes(index, config));
    output.into()
}

pub fn digest_word(hash: &[u8; 64]) -> u64 {
    hash.first_chunk::<8>()
        .map_or(0, |word| u64::from_be_bytes(*word))
}

pub const fn matches_leading_zero_bits(word: u64, bits: u32) -> bool {
    if bits == 0 {
        return true;
    }
    bits <= 64 && (word >> (64 - bits)) == 0
}

pub fn bytes_to_uuid_string(bytes: &[u8; 16]) -> String {
    let [
        b0,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
        b8,
        b9,
        b10,
        b11,
        b12,
        b13,
        b14,
        b15,
    ] = *bytes;
    format!(
        "{b0:02x}{b1:02x}{b2:02x}{b3:02x}-{b4:02x}{b5:02x}-{b6:02x}{b7:02x}-{b8:02x}{b9:02x}-{b10:02x}{b11:02x}{b12:02x}{b13:02x}{b14:02x}{b15:02x}"
    )
}

#[cfg(test)]
mod test;
