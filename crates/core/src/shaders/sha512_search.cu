extern "C" {

__device__ __forceinline__ unsigned long long rotr(unsigned long long value, int amount) {
    return (value >> amount) | (value << (64 - amount));
}

__device__ __forceinline__ unsigned long long big_sigma0(unsigned long long value) {
    return rotr(value, 28) ^ rotr(value, 34) ^ rotr(value, 39);
}

__device__ __forceinline__ unsigned long long big_sigma1(unsigned long long value) {
    return rotr(value, 14) ^ rotr(value, 18) ^ rotr(value, 41);
}

__device__ __forceinline__ unsigned long long small_sigma0(unsigned long long value) {
    return rotr(value, 1) ^ rotr(value, 8) ^ (value >> 7);
}

__device__ __forceinline__ unsigned long long small_sigma1(unsigned long long value) {
    return rotr(value, 19) ^ rotr(value, 61) ^ (value >> 6);
}

struct CandidateWords {
    unsigned long long word0;
    unsigned long long word1;
};

__device__ __forceinline__ CandidateWords candidate_words(unsigned long long index, unsigned long long commit, unsigned long long node_suffix) {
    unsigned long long node_prefix = index & 0xFFFFull;
    unsigned long long variant_tail = (index >> 16) & 0xFFFull;
    unsigned long long variant_head = (index >> 28) & 0x3ull;
    unsigned long long time_high = (index >> 30) & 0xFFFull;
    unsigned long long time_mid = (index >> 42) & 0xFFFFull;

    unsigned long long word0 = (commit << 32) | (time_mid << 16) | 0x4000ull | time_high;
    unsigned long long variant = ((8ull + variant_head) << 12) | variant_tail;
    unsigned long long word1 = (variant << 48) | (node_prefix << 32) | node_suffix;

    return CandidateWords{word0, word1};
}

__device__ __forceinline__ unsigned long long sha512_first_word(unsigned long long index, unsigned long long commit, unsigned long long node_suffix) {
    CandidateWords candidate = candidate_words(index, commit, node_suffix);
    unsigned long long w0 = candidate.word0;
    unsigned long long w1 = candidate.word1;
    unsigned long long w2 = 0x8000000000000000ull;
    unsigned long long w3 = 0ull;
    unsigned long long w4 = 0ull;
    unsigned long long w5 = 0ull;
    unsigned long long w6 = 0ull;
    unsigned long long w7 = 0ull;
    unsigned long long w8 = 0ull;
    unsigned long long w9 = 0ull;
    unsigned long long w10 = 0ull;
    unsigned long long w11 = 0ull;
    unsigned long long w12 = 0ull;
    unsigned long long w13 = 0ull;
    unsigned long long w14 = 0ull;
    unsigned long long w15 = 128ull;

    unsigned long long a = 0x6a09e667f3bcc908ull;
    unsigned long long b = 0xbb67ae8584caa73bull;
    unsigned long long c = 0x3c6ef372fe94f82bull;
    unsigned long long d = 0xa54ff53a5f1d36f1ull;
    unsigned long long e = 0x510e527fade682d1ull;
    unsigned long long f = 0x9b05688c2b3e6c1full;
    unsigned long long g = 0x1f83d9abfb41bd6bull;
    unsigned long long h = 0x5be0cd19137e2179ull;

    // @SHA512_UNROLLED_ROUNDS@
}

__device__ __forceinline__ bool matches_leading_zero_bits(unsigned long long word, unsigned int leading_zero_bits) {
    if (leading_zero_bits == 0) {
        return true;
    }
    if (leading_zero_bits > 64) {
        return false;
    }
    return (word >> ((64 - leading_zero_bits) & 63)) == 0ull;
}
__global__ void search_main(
    unsigned int* result,
    const unsigned int* params,
    unsigned long long commit,
    unsigned long long node_suffix,
    unsigned int leading_zero_bits,
    unsigned int candidates_per_invocation
) {
    unsigned long long base = ((unsigned long long)params[1] << 32) | (unsigned long long)params[0];
    unsigned long long invocation_start = base + (unsigned long long)(blockIdx.x * blockDim.x + threadIdx.x) * (unsigned long long)candidates_per_invocation;

    unsigned int best_local_offset = 0xFFFFFFFFu;
    for (unsigned int offset = 0; offset < candidates_per_invocation; offset++) {
        unsigned long long local_index = (unsigned long long)(blockIdx.x * blockDim.x + threadIdx.x) * (unsigned long long)candidates_per_invocation + (unsigned long long)offset;
        if (local_index >= (unsigned long long)params[2]) {
            break;
        }

        unsigned long long index = invocation_start + (unsigned long long)offset;
        if (matches_leading_zero_bits(sha512_first_word(index, commit, node_suffix), leading_zero_bits)) {
            best_local_offset = offset;
            break;
        }
    }

    if (best_local_offset != 0xFFFFFFFFu) {
        unsigned int global_offset = (blockIdx.x * blockDim.x + threadIdx.x) * candidates_per_invocation + best_local_offset;
        atomicMin(result, global_offset);
    }
}

__global__ void hash_main(
    unsigned long long* hash_results,
    const unsigned int* params,
    unsigned long long commit,
    unsigned long long node_suffix
) {
    unsigned int gid = blockIdx.x * blockDim.x + threadIdx.x;
    if (gid >= params[2]) {
        return;
    }
    unsigned long long base = ((unsigned long long)params[1] << 32) | (unsigned long long)params[0];
    hash_results[gid] = sha512_first_word(base + (unsigned long long)gid, commit, node_suffix);
}

}
