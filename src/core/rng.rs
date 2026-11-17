use rand::{rngs::StdRng, SeedableRng};

/// Deterministically derive a seed from a base seed, an entity-level salt, and an
/// entity index.
///
/// Mirrors the `base ^ salt ^ index` pattern the generators use, so per-entity
/// streams stay stable and independent of iteration order (and therefore of thread
/// scheduling under `rayon`).
pub fn derive_seed(base: u64, salt: u64, index: u64) -> u64 {
    base ^ salt ^ index
}

/// A deterministic RNG stream for one entity, seeded via [`derive_seed`].
pub fn stream(base: u64, salt: u64, index: u64) -> StdRng {
    StdRng::seed_from_u64(derive_seed(base, salt, index))
}
