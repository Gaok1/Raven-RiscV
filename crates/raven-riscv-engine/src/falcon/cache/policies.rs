// falcon/cache/policies.rs â€” Replacement, write, and inclusion policy enums

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementPolicy {
    /// Least Recently Used â€” evicts the way not accessed for longest
    Lru,
    /// First In First Out â€” evicts oldest installed line
    Fifo,
    /// Pseudo-random via LCG
    Random,
    /// Least Frequently Used â€” evicts way with fewest accesses
    Lfu,
    /// Clock (Second Chance) â€” circular pointer with reference bit
    Clock,
    /// Most Recently Used â€” evicts most recently accessed (good for scans)
    Mru,
}

impl ReplacementPolicy {
    /// Stable lowercase tag used by the config serializer/parser.  Rename
    /// variants without breaking saved `.fcache` / `.rcfg` files.
    pub fn as_str(self) -> &'static str {
        match self {
            ReplacementPolicy::Lru => "lru",
            ReplacementPolicy::Fifo => "fifo",
            ReplacementPolicy::Random => "random",
            ReplacementPolicy::Lfu => "lfu",
            ReplacementPolicy::Clock => "clock",
            ReplacementPolicy::Mru => "mru",
        }
    }

    /// Inverse of [`as_str`]. Returns `None` on unknown tags so the caller
    /// can surface a real error instead of silently falling back to LRU.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lru" => Some(ReplacementPolicy::Lru),
            "fifo" => Some(ReplacementPolicy::Fifo),
            "random" => Some(ReplacementPolicy::Random),
            "lfu" => Some(ReplacementPolicy::Lfu),
            "clock" => Some(ReplacementPolicy::Clock),
            "mru" => Some(ReplacementPolicy::Mru),
            _ => None,
        }
    }
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
    /// No constraint â€” data may or may not appear in both levels (NINE).
    #[default]
    NonInclusive,
    /// Every line in this level is guaranteed to also exist in the level below.
    Inclusive,
    /// Lines in this level are guaranteed NOT to exist in the level below.
    Exclusive,
}

