//! The in-repo deterministic pseudorandom stream
//! (025-exploratory-parity-discovery, research D2, T010/T011).
//!
//! SplitMix64 seed expansion feeding a xoshiro256\*\* stream — roughly forty lines of
//! wrapping integer arithmetic and rotations, no `unsafe`, no new dependency.
//!
//! ## Why this is in-repo rather than the `rand` crate
//!
//! FR-001 requires a recorded seed to reproduce an identical candidate sequence, and
//! FR-034 requires findings to persist indefinitely — so a seed recorded today must
//! still reproduce after arbitrary dependency updates. `rand` explicitly does **not**
//! offer value-stream stability across versions; its own documentation treats the stream
//! as an implementation detail. Depending on it would make every finding's
//! reproducibility hostage to a `cargo update`, and a security advisory could force a
//! bump that silently invalidates the entire recorded corpus with no signal.
//!
//! Making the stream a property of *our* committed code inverts that: the algorithm
//! identity becomes an explicit component of `generatorVersion` (the seventh element of
//! the pinned input set, data-model.md § 4), so a deliberate change is a recorded,
//! reviewable pin change — exactly like `NORMALIZER_VERSION`, and for the identical
//! reason.
//!
//! ## What makes the stream a *pin* rather than an accident
//!
//! The unit tests below check the stream against **published reference vectors** for
//! both stages. Without them, "the stream is stable" would be an assertion about code
//! nobody compared to anything; with them, an accidental edit to a shift constant fails
//! a named test instead of silently re-rolling every campaign ever recorded.
//!
//! ## What counts as a change to `generatorVersion`
//!
//! Everything in this module that can move a draw: the two stages' constants, the order
//! in which [`Prng::from_seed`] expands the seed into state, and the derivation of every
//! helper below ([`Prng::next_bounded`], [`Prng::choose`], [`Prng::shuffle`], …). A
//! "harmless refactor" of `next_bounded`'s rejection loop changes which candidates a
//! seed produces, so it is a pin change and must bump [`PRNG_VERSION`].

/// The algorithm identity of the stream — one half of the `generatorVersion` element of
/// the pinned input set (data-model.md § 4). The other half is the reduction
/// catalogue's *order*, which lives with the shrinker (data-model.md § 6).
///
/// Both are reproducibility-critical — FR-001 depends on the stream, FR-020 on the
/// order — and folding either into `mutationCatalogVersion` would name it for something
/// it is not, so a deliberate change to reduction order would look like a change to the
/// mutation operators.
pub const PRNG_ALGORITHM: &str = "splitmix64-seed+xoshiro256starstar";

/// The revision of *this module's* derivation, bumped whenever any draw could move.
///
/// Distinct from [`PRNG_ALGORITHM`] because the algorithm can stay xoshiro256\*\* while
/// a helper's derivation changes: both determine the candidate sequence, only one is
/// named by the algorithm string.
pub const PRNG_VERSION: u32 = 1;

/// The composed algorithm identity recorded in a campaign's `generatorVersion`.
pub fn prng_identity() -> String {
    format!("{PRNG_ALGORITHM}/v{PRNG_VERSION}")
}

/// SplitMix64 — the seed expander (Steele, Lea & Flood 2014).
///
/// Used **only** to turn a single 64-bit seed into xoshiro256\*\*'s 256-bit state, which
/// is the expansion the xoshiro reference implementation itself recommends: a
/// poorly-distributed state (all-zero, or nearly so) makes xoshiro's output correlated
/// for a long prefix, and a user-chosen seed like `0x5eed1234` is exactly that.
///
/// The expansion also makes an all-zero state — xoshiro's one fixed point, from which it
/// never escapes — unreachable: SplitMix64 is a bijection on 64 bits, so it maps exactly
/// one input to zero, and four *consecutive* counter values can therefore never all map
/// to zero. No runtime guard is needed, and adding one would be a divergence from the
/// reference for a state that cannot occur.
#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Rotate-left, the one primitive both stages need.
#[inline]
fn rotl(x: u64, k: u32) -> u64 {
    x.rotate_left(k)
}

/// The campaign pseudorandom stream: xoshiro256\*\* over a SplitMix64-expanded seed.
///
/// Deterministic and self-contained. Two `Prng`s built from the same seed produce the
/// same sequence forever, on every platform and every toolchain, because nothing here
/// depends on pointer width, endianness, floating point, or hash-map iteration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prng {
    s: [u64; 4],
}

impl Prng {
    /// Expand a 64-bit seed into the 256-bit state via four SplitMix64 draws.
    ///
    /// The expansion order (`s[0]`, `s[1]`, `s[2]`, `s[3]`, in that order) is part of
    /// the pin: permuting it would produce a different, equally valid stream, and every
    /// finding recorded under the old order would silently stop reproducing.
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = SplitMix64::new(seed);
        Prng {
            s: [sm.next(), sm.next(), sm.next(), sm.next()],
        }
    }

    /// Construct directly from a raw 256-bit state.
    ///
    /// Exists so the reference vectors can be checked against the canonical published
    /// state `[1, 2, 3, 4]` — the only way to compare this implementation to the
    /// upstream test data, since that state is not reachable from any seed.
    ///
    /// An all-zero state is xoshiro's fixed point (it emits zeros forever), so it is
    /// rejected rather than accepted as a silently dead generator.
    pub fn from_state(s: [u64; 4]) -> Option<Self> {
        if s == [0; 4] { None } else { Some(Prng { s }) }
    }

    /// The next 64-bit draw (xoshiro256\*\*, Blackman & Vigna).
    pub fn next_u64(&mut self) -> u64 {
        let result = rotl(self.s[1].wrapping_mul(5), 7).wrapping_mul(9);
        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = rotl(self.s[3], 45);

        result
    }

    /// A uniform draw in `0..bound`, or `None` when `bound == 0` (an empty range has no
    /// member to return, and returning `0` would be a silent fallback).
    ///
    /// Uses Lemire's multiply-shift with the exact rejection threshold, so the result is
    /// **unbiased** — not merely "close enough". A modulo reduction would over-represent
    /// the low residues, which for a generator drawing from a constraint grammar means
    /// systematically over-exploring whichever alternatives happen to be declared first
    /// and under-exploring the rest. That is a coverage defect disguised as a rounding
    /// detail.
    ///
    /// The rejection loop terminates with probability 1 and, for every `bound`, expects
    /// well under two draws; it is not an unbounded search.
    pub fn next_bounded(&mut self, bound: u64) -> Option<u64> {
        if bound == 0 {
            return None;
        }
        let mut x = self.next_u64();
        let mut m = (x as u128).wrapping_mul(bound as u128);
        let mut low = m as u64;
        if low < bound {
            // Reject the short tail so every residue is equally likely.
            let threshold = bound.wrapping_neg() % bound;
            while low < threshold {
                x = self.next_u64();
                m = (x as u128).wrapping_mul(bound as u128);
                low = m as u64;
            }
        }
        Some((m >> 64) as u64)
    }

    /// A uniform index into a slice of `len` elements, or `None` when it is empty.
    pub fn next_index(&mut self, len: usize) -> Option<usize> {
        self.next_bounded(len as u64).map(|v| v as usize)
    }

    /// A uniform boolean (the stream's top bit — the best-distributed one for
    /// xoshiro\*\*, whose lowest bits are the weakest).
    pub fn next_bool(&mut self) -> bool {
        (self.next_u64() >> 63) != 0
    }

    /// A uniformly chosen element of `items`, or `None` when it is empty.
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        self.next_index(items.len()).map(|i| &items[i])
    }

    /// Fisher–Yates shuffle in place, iterating **downward** from the last index.
    ///
    /// The direction is part of the pin: the upward variant consumes the same draws in a
    /// different order and produces a different permutation from the same seed.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        if items.len() < 2 {
            return;
        }
        for i in (1..items.len()).rev() {
            // `i >= 1`, so the bound is at least 2 and `next_bounded` always yields.
            // Written as a `let ... else` rather than an `expect` so no runtime path can
            // panic (constitution V) — a shuffle that somehow could not draw leaves the
            // prefix unpermuted rather than aborting a campaign mid-run.
            let Some(j) = self.next_bounded((i + 1) as u64) else {
                return;
            };
            items.swap(i, j as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published SplitMix64 output sequence for seed `0`. These are the values every
    /// reference port reproduces; they also happen to be the four words xoshiro's own
    /// reference seeding produces for a zero seed.
    const SPLITMIX64_SEED_0: [u64; 6] = [
        0xE220_A839_7B1D_CDAF,
        0x6E78_9E6A_A1B9_65F4,
        0x06C4_5D18_8009_454F,
        0xF88B_B8A8_724C_81EC,
        0x1B39_896A_51A8_749B,
        0x53CB_9F0C_747E_A2EA,
    ];

    /// The published xoshiro256\*\* output sequence for the canonical test state
    /// `s = [1, 2, 3, 4]`. This is the vector that makes the stream a reviewable pin:
    /// an accidental edit to a shift, a rotation, or the state-update order fails here
    /// rather than silently re-rolling every campaign ever recorded.
    const XOSHIRO256SS_STATE_1234: [u64; 10] = [
        11520,
        0,
        1_509_978_240,
        1_215_971_899_390_074_240,
        1_216_172_134_540_287_360,
        607_988_272_756_665_600,
        16_172_922_978_634_559_625,
        8_476_171_486_693_032_832,
        10_595_114_339_597_558_777,
        2_904_607_092_377_533_576,
    ];

    #[test]
    fn splitmix64_matches_published_reference_vectors() {
        let mut sm = SplitMix64::new(0);
        for (i, expected) in SPLITMIX64_SEED_0.iter().enumerate() {
            let got = sm.next();
            assert_eq!(
                got, *expected,
                "SplitMix64(seed=0) draw {i}: got {got:#018x}, published {expected:#018x}"
            );
        }
    }

    #[test]
    fn xoshiro256ss_matches_published_reference_vectors() {
        let mut prng = Prng::from_state([1, 2, 3, 4]).expect("non-zero state");
        for (i, expected) in XOSHIRO256SS_STATE_1234.iter().enumerate() {
            let got = prng.next_u64();
            assert_eq!(
                got, *expected,
                "xoshiro256** from s=[1,2,3,4] draw {i}: got {got}, published {expected}"
            );
        }
    }

    #[test]
    fn seed_expansion_uses_splitmix64_in_declaration_order() {
        // The expansion order is part of the pin, so assert it directly rather than
        // only through the composed stream: a permuted expansion would still look
        // "random" but would break every recorded seed.
        let prng = Prng::from_seed(0);
        assert_eq!(
            prng.s,
            [
                SPLITMIX64_SEED_0[0],
                SPLITMIX64_SEED_0[1],
                SPLITMIX64_SEED_0[2],
                SPLITMIX64_SEED_0[3],
            ]
        );
    }

    #[test]
    fn the_same_seed_reproduces_the_same_sequence() {
        // FR-001 in miniature: this is the property every finding's reproducibility
        // rests on.
        let mut a = Prng::from_seed(0x5EED_1234);
        let mut b = Prng::from_seed(0x5EED_1234);
        let left: Vec<u64> = (0..64).map(|_| a.next_u64()).collect();
        let right: Vec<u64> = (0..64).map(|_| b.next_u64()).collect();
        assert_eq!(left, right);

        let mut other = Prng::from_seed(0x5EED_1235);
        let different: Vec<u64> = (0..64).map(|_| other.next_u64()).collect();
        assert_ne!(left, different, "a different seed must not alias");
    }

    #[test]
    fn an_all_zero_state_is_refused() {
        assert!(
            Prng::from_state([0, 0, 0, 0]).is_none(),
            "the all-zero state is xoshiro's fixed point — a generator that emits zeros \
             forever must never be constructible"
        );
        // No seed can reach it: SplitMix64 is a bijection, so four consecutive counter
        // values cannot all map to zero. Spot-check the pathological seeds anyway.
        for seed in [0u64, 1, u64::MAX, 0x9E37_79B9_7F4A_7C15] {
            assert_ne!(Prng::from_seed(seed).s, [0; 4], "seed {seed:#x}");
        }
    }

    #[test]
    fn next_bounded_stays_in_range_and_refuses_an_empty_range() {
        let mut prng = Prng::from_seed(7);
        assert_eq!(prng.next_bounded(0), None, "an empty range has no member");
        assert_eq!(prng.next_bounded(1), Some(0), "a singleton range is forced");
        for _ in 0..10_000 {
            let v = prng.next_bounded(7).expect("non-zero bound");
            assert!(v < 7, "draw {v} escaped the bound");
        }
    }

    #[test]
    fn next_bounded_is_close_to_uniform() {
        // Not a statistical proof — a smoke test that the rejection arithmetic is not
        // inverted. With 120_000 draws over 6 buckets the expected count is 20_000;
        // a modulo-style bias or an off-by-one in the threshold moves a bucket far
        // outside this window, while genuine sampling noise does not.
        let mut prng = Prng::from_seed(0xABCD_EF01);
        let mut counts = [0usize; 6];
        for _ in 0..120_000 {
            counts[prng.next_bounded(6).expect("non-zero bound") as usize] += 1;
        }
        for (bucket, count) in counts.iter().enumerate() {
            assert!(
                (18_500..=21_500).contains(count),
                "bucket {bucket} got {count} of 120000 draws (expected ~20000): {counts:?}"
            );
        }
    }

    #[test]
    fn choose_and_shuffle_are_seed_reproducible() {
        let items = ["a", "b", "c", "d", "e"];
        let mut a = Prng::from_seed(42);
        let mut b = Prng::from_seed(42);
        assert_eq!(a.choose(&items), b.choose(&items));

        let mut left = items;
        let mut right = items;
        let mut pa = Prng::from_seed(99);
        let mut pb = Prng::from_seed(99);
        pa.shuffle(&mut left);
        pb.shuffle(&mut right);
        assert_eq!(left, right, "the same seed must yield the same permutation");

        let mut sorted = left;
        sorted.sort_unstable();
        assert_eq!(
            sorted, items,
            "shuffle must be a permutation, not a rewrite"
        );

        let empty: [u8; 0] = [];
        assert_eq!(Prng::from_seed(1).choose(&empty), None);
    }

    #[test]
    fn shuffle_handles_degenerate_lengths() {
        let mut prng = Prng::from_seed(3);
        let mut empty: [u8; 0] = [];
        prng.shuffle(&mut empty);
        let mut single = [7u8];
        prng.shuffle(&mut single);
        assert_eq!(single, [7]);
    }

    #[test]
    fn the_identity_string_names_the_algorithm_and_the_revision() {
        assert_eq!(
            prng_identity(),
            "splitmix64-seed+xoshiro256starstar/v1",
            "generatorVersion's PRNG component is a reviewed pin — changing it is a \
             deliberate act, not a refactor"
        );
    }
}
