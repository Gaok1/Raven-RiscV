// falcon/cache/policies.rs — Replacement, write, and inclusion policy enums

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementPolicy {
    /// Least Recently Used — evicts the way not accessed for longest
    Lru,
    /// First In First Out — evicts oldest installed line
    Fifo,
    /// Pseudo-random via LCG
    Random,
    /// Least Frequently Used — evicts way with fewest accesses
    Lfu,
    /// Clock (Second Chance) — circular pointer with reference bit
    Clock,
    /// Most Recently Used — evicts most recently accessed (good for scans)
    Mru,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteAllocPolicy {
    WriteAllocate,
    NoWriteAllocate,
}

/// Inclusion policy between this cache level and the NEXT level below it.
/// Applies to all levels except the last (which has no level below it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InclusionPolicy {
    /// No constraint — data may or may not appear in both levels (NINE).
    #[default]
    NonInclusive,
    /// Every line in this level is guaranteed to also exist in the level below.
    Inclusive,
    /// Lines in this level are guaranteed NOT to exist in the level below.
    Exclusive,
}
