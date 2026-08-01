//! Inspecting a machine's cache hierarchy.
//!
//! Cache contents are observer state: drawing them must not perform an access,
//! update replacement metadata, or move a counter. The borrowed line data and
//! history slices keep that inspection cheap even when a host redraws every
//! frame.

/// The purpose a cache serves within one hierarchy level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheRole {
    Instruction,
    Data,
    Unified,
}

/// The replacement behaviors a host can explain alongside cache contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheReplacementPolicy {
    Lru,
    Mru,
    Fifo,
    Random,
    Lfu,
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheWritePolicy {
    WriteThrough,
    WriteBack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheWriteAllocation {
    WriteAllocate,
    NoWriteAllocate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheInclusionPolicy {
    NonInclusive,
    Inclusive,
    Exclusive,
}

/// Configuration needed to describe a cache without exposing a backend's
/// address or word type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheLevelConfig {
    pub size: usize,
    pub line_size: usize,
    pub associativity: usize,
    pub num_sets: usize,
    pub address_bits: usize,
    pub replacement: CacheReplacementPolicy,
    pub write_policy: CacheWritePolicy,
    pub write_allocation: CacheWriteAllocation,
    pub inclusion: CacheInclusionPolicy,
    pub hit_latency: u64,
    pub miss_penalty: u64,
    pub associativity_penalty: u64,
    pub transfer_width: usize,
    pub enabled: bool,
}

impl CacheLevelConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn num_sets(&self) -> usize {
        self.num_sets
    }

    pub fn offset_bits(&self) -> usize {
        self.line_size.trailing_zeros() as usize
    }

    pub fn index_bits(&self) -> usize {
        self.num_sets.trailing_zeros() as usize
    }

    pub fn tag_search_cycles(&self) -> u64 {
        self.hit_latency
            + (self.associativity as u64).saturating_sub(1) * self.associativity_penalty
    }

    pub fn line_transfer_cycles(&self) -> u64 {
        let width = self.transfer_width.max(1) as u64;
        (self.line_size as u64).div_ceil(width)
    }
}

/// Borrowed history storage. `VecDeque` may wrap, so both slices are exposed
/// through one iterator rather than copied into a temporary vector.
#[derive(Clone, Copy, Debug)]
pub struct CacheHistory<'a> {
    first: &'a [(f64, f64)],
    second: &'a [(f64, f64)],
}

impl<'a> CacheHistory<'a> {
    pub fn new(first: &'a [(f64, f64)], second: &'a [(f64, f64)]) -> Self {
        Self { first, second }
    }

    pub fn len(&self) -> usize {
        self.first.len() + self.second.len()
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_empty() && self.second.is_empty()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &'a (f64, f64)> + Clone {
        self.first.iter().chain(self.second.iter())
    }
}

/// Per-cache counters copied by value, with the potentially larger history
/// retained as borrowed storage.
#[derive(Clone, Copy, Debug)]
pub struct CacheLevelStats<'a> {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub writebacks: u64,
    pub bytes_loaded: u64,
    pub bytes_stored: u64,
    pub total_cycles: u64,
    pub ram_write_bytes: u64,
    pub history: CacheHistory<'a>,
}

impl CacheLevelStats<'_> {
    pub fn total_accesses(&self) -> u64 {
        self.hits + self.misses
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.total_accesses();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64 * 100.0
        }
    }

    pub fn mpki(&self, instructions: u64) -> f64 {
        if instructions == 0 {
            0.0
        } else {
            self.misses as f64 / instructions as f64 * 1000.0
        }
    }
}

/// One cache within a hierarchy level. Split L1 caches have distinct roles;
/// lower shared levels report [`CacheRole::Unified`].
#[derive(Clone, Debug)]
pub struct CacheLevelView<'a> {
    pub name: String,
    pub level: usize,
    pub role: CacheRole,
    pub config: CacheLevelConfig,
    pub stats: CacheLevelStats<'a>,
}

/// A line borrowed directly from the simulated cache.
#[derive(Clone, Copy, Debug)]
pub struct CacheLineView<'a> {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u64,
    pub data: &'a [u8],
    pub frequency: u64,
    pub referenced: bool,
}

/// Replacement metadata for one set. Line bytes remain borrowed; only the
/// small ordering vectors are copied because deque storage may wrap.
#[derive(Clone, Debug)]
pub struct CacheSetView<'a> {
    pub lines: Vec<CacheLineView<'a>>,
    pub lru_order: Vec<usize>,
    pub fifo_order: Vec<usize>,
    pub clock_hand: usize,
}

/// Read-only cache information suitable for statistics and content panes.
pub trait CacheHierarchy {
    /// Number of hierarchy levels, counting a split L1 only once.
    fn level_count(&self) -> usize;

    /// Describe the cache serving `role` at zero-based `level`.
    fn cache(&self, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>>;

    /// Borrow one set for a content pane without performing a cache access.
    fn set(&self, level: usize, role: CacheRole, set: usize) -> Option<CacheSetView<'_>>;

    /// Whether a host may let the user retune this hierarchy — add or remove
    /// levels, change associativity, swap the replacement policy.
    ///
    /// A fixed teaching cache says `false`, and the host hides the editing
    /// controls rather than offering buttons that cannot take effect.
    fn is_configurable(&self) -> bool {
        false
    }

    /// Cycles this hierarchy has charged the program, across every level and
    /// role.
    ///
    /// The default sums what the levels report. A backend that tracks the
    /// number directly — because it also charges for writebacks or prefetches
    /// no single level owns — overrides it.
    fn total_cycles(&self) -> u64 {
        (0..self.level_count())
            .flat_map(|level| {
                [CacheRole::Instruction, CacheRole::Data, CacheRole::Unified]
                    .into_iter()
                    .filter_map(move |role| self.cache(level, role))
            })
            .map(|cache| cache.stats.total_cycles)
            .sum()
    }

    /// Average memory access time for one cache, in cycles.
    ///
    /// The default is the single-level formula, `hit + miss_rate × penalty`. A
    /// backend with deeper levels overrides it, because a miss there costs the
    /// next level's AMAT rather than a flat penalty — and only the hierarchy
    /// knows how its levels chain.
    fn amat(&self, level: usize, role: CacheRole) -> f64 {
        let Some(cache) = self.cache(level, role) else {
            return 0.0;
        };
        let total = cache.stats.total_accesses();
        let miss_rate = if total == 0 {
            0.0
        } else {
            cache.stats.misses as f64 / total as f64
        };
        cache.config.hit_latency as f64 + miss_rate * cache.config.miss_penalty as f64
    }

    /// A display name for `level` — `"L1"`, `"L2"`, … — so a host labels a
    /// hierarchy of any depth without a naming table of its own.
    fn level_name(&self, level: usize) -> String {
        [CacheRole::Unified, CacheRole::Instruction, CacheRole::Data]
            .into_iter()
            .find_map(|role| self.cache(level, role))
            .map_or_else(|| format!("L{}", level + 1), |cache| cache.name.to_string())
    }
}
