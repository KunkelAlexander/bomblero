/// A tiny deterministic random number generator (xorshift32).
///
/// Why our own instead of the `rand` crate: it is 10 lines, behaves
/// identically on native and WASM, and a given seed always gives the same
/// sequence. Determinism makes tests reliable now and would let a server and
/// clients agree on "random" results later.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1)) // xorshift gets stuck at 0
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}
