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
    /// The line's state under the coherence protocol, when one tracks it.
    /// `None` for a cache no protocol watches: a single shared L1, a private
    /// one with coherence off, or any lower level.
    pub state: Option<CoherenceState>,
}

/// The state a coherence protocol keeps for one line in one private cache.
///
/// The names are the MOESI ones; MSI uses three of them and MESI four.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoherenceState {
    /// The only valid copy, and newer than memory.
    Modified,
    /// Newer than memory, shared with other caches that hold it in `Shared`.
    /// This cache answers for the line and writes it back when it leaves.
    Owned,
    /// The only copy, and the same as memory: a write needs no bus traffic.
    Exclusive,
    /// One of possibly several read-only copies.
    Shared,
    /// Not held.
    Invalid,
}

impl CoherenceState {
    /// The one-letter name diagrams use: `M`, `O`, `E`, `S`, `I`.
    pub fn letter(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Owned => 'O',
            Self::Exclusive => 'E',
            Self::Shared => 'S',
            Self::Invalid => 'I',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Owned => "Owned",
            Self::Exclusive => "Exclusive",
            Self::Shared => "Shared",
            Self::Invalid => "Invalid",
        }
    }
}

/// What one core's private caches did on the coherence bus, and what the
/// others did to them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoherenceCoreStats {
    /// Read misses broadcast as `BusRd`.
    pub bus_reads: u64,
    /// Write misses broadcast as `BusRdX` (read for ownership).
    pub bus_read_exclusive: u64,
    /// Writes to a line held `Shared` or `Owned`, broadcast as `BusUpgr`.
    pub bus_upgrades: u64,
    /// Writes put on the bus as they are: write-through stores and
    /// no-write-allocate misses.
    pub bus_writes: u64,
    /// Lines of this core another core's request invalidated.
    pub invalidations_received: u64,
    /// Dirty lines this core wrote back to the shared level because another
    /// core asked for them.
    pub flushes: u64,
    /// Lines this core sent straight to another core's cache.
    pub cache_to_cache: u64,
    /// Misses on a line another core's write had invalidated here: the cost
    /// of sharing, as opposed to capacity or conflict.
    pub coherence_misses: u64,
    /// Cycles this core waited on the bus for snoops and transfers.
    pub bus_cycles: u64,
}

impl CoherenceCoreStats {
    /// Requests this core put on the bus.
    pub fn transactions(&self) -> u64 {
        self.bus_reads + self.bus_read_exclusive + self.bus_upgrades + self.bus_writes
    }
}

/// The bus request behind a [`CoherenceEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoherenceRequest {
    /// Read miss.
    BusRd,
    /// Write miss: read the line and invalidate every other copy.
    BusRdX,
    /// Write hit on a shared line: invalidate every other copy.
    BusUpgr,
    /// A write that goes on the bus as it is (write-through, no allocate).
    BusWr,
    /// An instruction fetch miss, which makes a dirty data copy elsewhere
    /// write itself back.
    IFetch,
    /// A dirty line leaving its cache on eviction or `fence.i`.
    WriteBack,
}

impl CoherenceRequest {
    pub fn name(self) -> &'static str {
        match self {
            Self::BusRd => "BusRd",
            Self::BusRdX => "BusRdX",
            Self::BusUpgr => "BusUpgr",
            Self::BusWr => "BusWr",
            Self::IFetch => "IFetch",
            Self::WriteBack => "WriteBack",
        }
    }
}

/// One core's line changing state because of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoherenceTransition {
    pub core: usize,
    pub from: CoherenceState,
    pub to: CoherenceState,
}

/// One bus request and what it did to every cache that held the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoherenceEvent {
    /// The instruction count when it happened.
    pub instruction: u64,
    /// The core that made the request.
    pub core: usize,
    pub request: CoherenceRequest,
    /// Base address of the line.
    pub line: u64,
    /// The requester's own transition first, then each other core's.
    pub transitions: Vec<CoherenceTransition>,
    /// The core whose cache sent the data, when it did not come from the
    /// shared level.
    pub supplied_by: Option<usize>,
    /// Whether a dirty copy elsewhere had to be written back first.
    pub flushed: bool,
}

/// The coherence side of a hierarchy with one private L1 per core.
#[derive(Clone, Debug)]
pub struct CoherenceView<'a> {
    /// The protocol's short name: `"MESI"`, `"none"`, …
    pub protocol: &'static str,
    /// What the protocol does, in a sentence.
    pub description: &'static str,
    /// Whether the private caches keep each other up to date. `false` for
    /// private caches with coherence off, which is there to show what goes
    /// wrong without it.
    pub coherent: bool,
    /// The states this protocol uses, in diagram order.
    pub states: &'static [CoherenceState],
    pub snoop_latency: u64,
    pub cache_to_cache_latency: u64,
    /// One entry per core that has private caches.
    pub cores: &'a [CoherenceCoreStats],
    /// The most recent requests, oldest first.
    pub events: &'a std::collections::VecDeque<CoherenceEvent>,
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

    /// Number of cores with their own private L1 caches. `1` when every core
    /// shares one L1, as a hierarchy with no coherence protocol does.
    fn core_count(&self) -> usize {
        1
    }

    /// [`Self::cache`] as `core` sees it. Only level 0 differs between cores;
    /// the lower levels are shared. The default ignores `core`.
    fn core_cache(&self, _core: usize, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>> {
        self.cache(level, role)
    }

    /// [`Self::set`] as `core` sees it.
    fn core_set(
        &self,
        _core: usize,
        level: usize,
        role: CacheRole,
        set: usize,
    ) -> Option<CacheSetView<'_>> {
        self.set(level, role, set)
    }

    /// [`Self::amat`] as `core` sees it.
    fn core_amat(&self, _core: usize, level: usize, role: CacheRole) -> f64 {
        self.amat(level, role)
    }

    /// The coherence protocol between the cores' private caches, or `None`
    /// when the cores share one L1.
    fn coherence(&self) -> Option<CoherenceView<'_>> {
        None
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
