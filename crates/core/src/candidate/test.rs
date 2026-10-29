use super::*;

const CONFIG: CandidateConfig = CandidateConfig {
    commit: 0xeb36_6895,
    node_suffix: 0xebac_62b9,
};

#[test]
fn zero_index_matches_uuid_layout() {
    assert_eq!(
        candidate_bytes(0, CONFIG),
        [
            0xeb, 0x36, 0x68, 0x95, 0x00, 0x00, 0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0xeb, 0xac,
            0x62, 0xb9,
        ]
    );
}

#[test]
fn words_are_big_endian_candidate_bytes() {
    let indices = [
        0,
        0xFFFF,
        1 << 16,
        1 << 28,
        1 << 30,
        1 << 42,
        MAX_UUID_INDEX - 1,
    ];
    for index in indices {
        let words = candidate_words(index, CONFIG);
        let bytes = candidate_bytes(index, CONFIG);
        assert_eq!(&bytes[..8], &words[0].to_be_bytes());
        assert_eq!(&bytes[8..], &words[1].to_be_bytes());
    }
}

#[test]
fn leading_zero_predicate_handles_boundaries() {
    assert!(matches_leading_zero_bits(0, 64));
    assert!(matches_leading_zero_bits(1, 63));
    assert!(!matches_leading_zero_bits(2, 63));
    assert!(matches_leading_zero_bits(u64::MAX, 0));
    assert!(!matches_leading_zero_bits(u64::MAX, 1));
    assert!(!matches_leading_zero_bits(0, 65));
}
