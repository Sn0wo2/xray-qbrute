const SHA512_K: array<u64, 80> = array<u64, 80>(
    0x428a2f98d728ae22lu, 0x7137449123ef65cdlu,
    0xb5c0fbcfec4d3b2flu, 0xe9b5dba58189dbbclu,
    0x3956c25bf348b538lu, 0x59f111f1b605d019lu,
    0x923f82a4af194f9blu, 0xab1c5ed5da6d8118lu,
    0xd807aa98a3030242lu, 0x12835b0145706fbelu,
    0x243185be4ee4b28clu, 0x550c7dc3d5ffb4e2lu,
    0x72be5d74f27b896flu, 0x80deb1fe3b1696b1lu,
    0x9bdc06a725c71235lu, 0xc19bf174cf692694lu,
    0xe49b69c19ef14ad2lu, 0xefbe4786384f25e3lu,
    0x0fc19dc68b8cd5b5lu, 0x240ca1cc77ac9c65lu,
    0x2de92c6f592b0275lu, 0x4a7484aa6ea6e483lu,
    0x5cb0a9dcbd41fbd4lu, 0x76f988da831153b5lu,
    0x983e5152ee66dfablu, 0xa831c66d2db43210lu,
    0xb00327c898fb213flu, 0xbf597fc7beef0ee4lu,
    0xc6e00bf33da88fc2lu, 0xd5a79147930aa725lu,
    0x06ca6351e003826flu, 0x142929670a0e6e70lu,
    0x27b70a8546d22ffclu, 0x2e1b21385c26c926lu,
    0x4d2c6dfc5ac42aedlu, 0x53380d139d95b3dflu,
    0x650a73548baf63delu, 0x766a0abb3c77b2a8lu,
    0x81c2c92e47edaee6lu, 0x92722c851482353blu,
    0xa2bfe8a14cf10364lu, 0xa81a664bbc423001lu,
    0xc24b8b70d0f89791lu, 0xc76c51a30654be30lu,
    0xd192e819d6ef5218lu, 0xd69906245565a910lu,
    0xf40e35855771202alu, 0x106aa07032bbd1b8lu,
    0x19a4c116b8d2d0c8lu, 0x1e376c085141ab53lu,
    0x2748774cdf8eeb99lu, 0x34b0bcb5e19b48a8lu,
    0x391c0cb3c5c95a63lu, 0x4ed8aa4ae3418acblu,
    0x5b9cca4f7763e373lu, 0x682e6ff3d6b2b8a3lu,
    0x748f82ee5defb2fclu, 0x78a5636f43172f60lu,
    0x84c87814a1f0ab72lu, 0x8cc702081a6439eclu,
    0x90befffa23631e28lu, 0xa4506cebde82bde9lu,
    0xbef9a3f7b2c67915lu, 0xc67178f2e372532blu,
    0xca273eceea26619clu, 0xd186b8c721c0c207lu,
    0xeada7dd6cde0eb1elu, 0xf57d4f7fee6ed178lu,
    0x06f067aa72176fbalu, 0x0a637dc5a2c898a6lu,
    0x113f9804bef90daelu, 0x1b710b35131c471blu,
    0x28db77f523047d84lu, 0x32caab7b40c72493lu,
    0x3c9ebe0a15c9bebclu, 0x431d67c49c100d4clu,
    0x4cc5d4becb3e42b6lu, 0x597f299cfc657e2alu,
    0x5fcb6fab3ad6faeclu, 0x6c44198c4a475817lu,
);

struct Params {
    base_lo: u32,
    base_hi: u32,
    commit: u32,
    node_suffix: u32,
    candidate_count: u32,
    leading_zero_bits: u32,
    reserved0: u32,
    reserved1: u32,
};

struct SearchResult {
    claimed: atomic<u32>,
    index_lo: u32,
    index_hi: u32,
    reserved: u32,
};

@group(0) @binding(0)
var<storage, read> params: Params;

@group(0) @binding(1)
var<storage, read_write> search_result: SearchResult;

@group(0) @binding(2)
var<storage, read_write> hash_results: array<u64>;

fn rotr(value: u64, amount: u32) -> u64 {
    return (value >> amount) | (value << (64u - amount));
}

fn big_sigma0(value: u64) -> u64 {
    return rotr(value, 28u) ^ rotr(value, 34u) ^ rotr(value, 39u);
}

fn big_sigma1(value: u64) -> u64 {
    return rotr(value, 14u) ^ rotr(value, 18u) ^ rotr(value, 41u);
}

fn small_sigma0(value: u64) -> u64 {
    return rotr(value, 1u) ^ rotr(value, 8u) ^ (value >> 7u);
}

fn small_sigma1(value: u64) -> u64 {
    return rotr(value, 19u) ^ rotr(value, 61u) ^ (value >> 6u);
}

fn candidate_words(index: u64) -> vec2<u64> {
    let node_prefix = index & 0xfffflu;
    let variant_tail = (index >> 16u) & 0xffflu;
    let variant_head = (index >> 28u) & 0x3lu;
    let time_high = (index >> 30u) & 0xffflu;
    let time_mid = (index >> 42u) & 0xfffflu;

    let word0 = (u64(params.commit) << 32u) | (time_mid << 16u) | 0x4000lu | time_high;
    let variant = ((8lu + variant_head) << 12u) | variant_tail;
    let word1 = (variant << 48u) | (node_prefix << 32u) | u64(params.node_suffix);
    return vec2<u64>(word0, word1);
}

fn sha512_first_word(index: u64) -> u64 {
    let candidate = candidate_words(index);
    var schedule: array<u64, 16>;
    schedule[0] = candidate.x;
    schedule[1] = candidate.y;
    schedule[2] = 0x8000000000000000lu;
    schedule[15] = 128lu;

    var a = 0x6a09e667f3bcc908lu;
    var b = 0xbb67ae8584caa73blu;
    var c = 0x3c6ef372fe94f82blu;
    var d = 0xa54ff53a5f1d36f1lu;
    var e = 0x510e527fade682d1lu;
    var f = 0x9b05688c2b3e6c1flu;
    var g = 0x1f83d9abfb41bd6blu;
    var h = 0x5be0cd19137e2179lu;

    for (var round = 0u; round < 80u; round = round + 1u) {
        let slot = round & 15u;
        var word: u64;
        if (round < 16u) {
            word = schedule[round];
        } else {
            word = small_sigma1(schedule[(round + 14u) & 15u])
                + schedule[(round + 9u) & 15u]
                + small_sigma0(schedule[(round + 1u) & 15u])
                + schedule[slot];
            schedule[slot] = word;
        }

        let ch = (e & f) ^ ((~e) & g);
        let temp1 = h + big_sigma1(e) + ch + SHA512_K[round] + word;
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = big_sigma0(a) + maj;

        h = g;
        g = f;
        f = e;
        e = d + temp1;
        d = c;
        c = b;
        b = a;
        a = temp1 + temp2;
    }

    return a + 0x6a09e667f3bcc908lu;
}

fn matches_leading_zero_bits(word: u64, bits: u32) -> bool {
    if (bits == 0u) {
        return true;
    }
    if (bits > 64u) {
        return false;
    }
    return (word >> (64u - bits)) == 0lu;
}

@compute @workgroup_size(256)
fn search_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let base = (u64(params.base_hi) << 32u) | u64(params.base_lo);
    let invocation_start = base + u64(gid.x) * 32lu;

    for (var offset = 0u; offset < 32u; offset = offset + 1u) {
        let local_index = gid.x * 32u + offset;
        if (local_index >= params.candidate_count) {
            break;
        }

        let index = invocation_start + u64(offset);
        if (matches_leading_zero_bits(sha512_first_word(index), params.leading_zero_bits)) {
            let claim = atomicCompareExchangeWeak(&search_result.claimed, 0u, 1u);
            if (claim.exchanged) {
                search_result.index_lo = u32(index & 0xfffffffflu);
                search_result.index_hi = u32(index >> 32u);
                return;
            }
        }
    }
}

@compute @workgroup_size(256)
fn hash_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.candidate_count) {
        return;
    }
    let base = (u64(params.base_hi) << 32u) | u64(params.base_lo);
    hash_results[gid.x] = sha512_first_word(base + u64(gid.x));
}
