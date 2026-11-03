// falcon/cache/coherence.rs — the protocol that keeps private L1 caches coherent
//
// With `CoherenceProtocol::Shared` every core uses the one L1 the controller
// has always had, and nothing here runs. Any other protocol gives each core
// its own L1 I-cache and D-cache in front of the shared L2+ and RAM, and a
// snooping bus between the D-caches: every miss, and every write to a line
// another cache may hold, is a request the other caches see and answer.
//
// A line's state is not stored as such. It follows from three bits the line
// already carries or now carries:
//
//   valid  dirty  exclusive   state
//   false    -        -         I
//   true   true     true        M
//   true   true     false       O   (MOESI only)
//   true   false    true        E   (MESI and MOESI)
//   true   false    false       S
//
// so the code that only knows valid and dirty — eviction, write-back, the
// replacement policies — keeps working unchanged.

use std::collections::VecDeque;

use crate::capability::{
    CoherenceCoreStats, CoherenceEvent, CoherenceRequest, CoherenceState, CoherenceTransition,
};

use super::cache::CacheLine;

/// How the cores' L1 caches relate to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoherenceProtocol {
    /// One L1 shared by every core: there is a single copy of each line, so
    /// there is nothing to keep coherent. What RAVEN always did.
    #[default]
    Shared,
    /// A private L1 per core with no protocol between them. A core keeps
    /// reading its own copy after another core changed the line: the stale
    /// data a protocol exists to prevent.
    None,
    /// Modified, Shared, Invalid.
    Msi,
    /// MSI plus Exclusive: a line read with no other copy around can be
    /// written without a bus request.
    Mesi,
    /// MESI plus Owned: a dirty line can be shared without writing it back,
    /// and its owner sends it to the other caches directly.
    Moesi,
}

impl CoherenceProtocol {
    pub const ALL: [Self; 5] = [Self::Shared, Self::None, Self::Msi, Self::Mesi, Self::Moesi];

    /// Stable lowercase tag used in `.rcfg` files and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::None => "none",
            Self::Msi => "msi",
            Self::Mesi => "mesi",
            Self::Moesi => "moesi",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "shared" => Some(Self::Shared),
            "none" | "off" => Some(Self::None),
            "msi" => Some(Self::Msi),
            "mesi" => Some(Self::Mesi),
            "moesi" => Some(Self::Moesi),
            _ => None,
        }
    }

    /// The name a screen shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Shared => "Shared L1",
            Self::None => "none",
            Self::Msi => "MSI",
            Self::Mesi => "MESI",
            Self::Moesi => "MOESI",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Shared => "Every core uses the same L1, so there is one copy of each line.",
            Self::None => {
                "Each core has its own L1 and nothing keeps them in step: a core can read \
                 stale data another core already changed."
            }
            Self::Msi => {
                "Snooping MSI: a read miss fetches a Shared copy, a write invalidates every \
                 other copy, and a Modified line is written back when another core asks for it."
            }
            Self::Mesi => {
                "Snooping MESI: like MSI, but a line read while no other core holds it is \
                 Exclusive, and writing it needs no bus request."
            }
            Self::Moesi => {
                "Snooping MOESI: like MESI, but a Modified line another core reads becomes \
                 Owned; it stays dirty and its owner sends the data to the other caches."
            }
        }
    }

    /// Whether each core has an L1 of its own.
    pub fn private_l1(self) -> bool {
        self != Self::Shared
    }

    /// Whether the private caches snoop each other.
    pub fn coherent(self) -> bool {
        matches!(self, Self::Msi | Self::Mesi | Self::Moesi)
    }

    pub fn has_exclusive(self) -> bool {
        matches!(self, Self::Mesi | Self::Moesi)
    }

    pub fn has_owned(self) -> bool {
        self == Self::Moesi
    }

    /// The states this protocol uses, in diagram order.
    pub fn states(self) -> &'static [CoherenceState] {
        use CoherenceState::*;
        match self {
            Self::Shared | Self::None => &[],
            Self::Msi => &[Modified, Shared, Invalid],
            Self::Mesi => &[Modified, Exclusive, Shared, Invalid],
            Self::Moesi => &[Modified, Owned, Exclusive, Shared, Invalid],
        }
    }
}

/// The protocol and what its bus costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoherenceConfig {
    pub protocol: CoherenceProtocol,
    /// Cycles a core waits for the other caches to answer a request it put on
    /// the bus.
    pub snoop_latency: u64,
    /// Cycles to move a line from one core's cache to another's (MOESI). It
    /// replaces the trip to the shared level.
    pub cache_to_cache_latency: u64,
}

impl Default for CoherenceConfig {
    fn default() -> Self {
        Self {
            protocol: CoherenceProtocol::Shared,
            snoop_latency: 4,
            cache_to_cache_latency: 10,
        }
    }
}

impl CoherenceConfig {
    /// The `[cache]` section lines for this config, as `.rcfg` files hold
    /// them.
    pub fn to_rcfg(&self) -> String {
        format!(
            "coherence={}\ncoherence.snoop_latency={}\ncoherence.c2c_latency={}\n",
            self.protocol.as_str(),
            self.snoop_latency,
            self.cache_to_cache_latency
        )
    }

    /// Read the coherence keys of a `[cache]` section. A missing key keeps
    /// its default, so a file from before coherence existed loads as one
    /// shared L1.
    pub fn from_rcfg(text: &str) -> Result<Self, String> {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            let number = || {
                value
                    .parse::<u64>()
                    .map_err(|_| format!("Invalid {key}: expected integer"))
            };
            match key.as_str() {
                "coherence" => {
                    config.protocol = CoherenceProtocol::from_str(value).ok_or_else(|| {
                        format!(
                            "Unknown coherence protocol: {value} (use shared, none, msi, mesi or moesi)"
                        )
                    })?
                }
                "coherence.snoop_latency" => config.snoop_latency = number()?,
                "coherence.c2c_latency" => config.cache_to_cache_latency = number()?,
                _ => {}
            }
        }
        Ok(config)
    }
}

/// The state of `line` under a protocol that tracks one.
pub(super) fn line_state(line: &CacheLine) -> CoherenceState {
    match (line.valid, line.dirty, line.exclusive) {
        (false, _, _) => CoherenceState::Invalid,
        (true, true, true) => CoherenceState::Modified,
        (true, true, false) => CoherenceState::Owned,
        (true, false, true) => CoherenceState::Exclusive,
        (true, false, false) => CoherenceState::Shared,
    }
}

/// Counters per core, and the recent requests the Cache tab lists.
#[derive(Clone, Debug, Default)]
pub struct CoherenceTracker {
    pub cores: Vec<CoherenceCoreStats>,
    pub events: VecDeque<CoherenceEvent>,
}

/// How many requests the tracker remembers.
pub const EVENT_LOG_LEN: usize = 64;

impl CoherenceTracker {
    pub fn core(&mut self, core: usize) -> &mut CoherenceCoreStats {
        if self.cores.len() <= core {
            self.cores.resize(core + 1, CoherenceCoreStats::default());
        }
        &mut self.cores[core]
    }

    pub fn reset(&mut self) {
        self.cores
            .iter_mut()
            .for_each(|c| *c = CoherenceCoreStats::default());
        self.events.clear();
    }

    pub fn log(
        &mut self,
        instruction: u64,
        core: usize,
        request: CoherenceRequest,
        line: u32,
        transitions: Vec<CoherenceTransition>,
        supplied_by: Option<usize>,
        flushed: bool,
    ) {
        if self.events.len() >= EVENT_LOG_LEN {
            self.events.pop_front();
        }
        self.events.push_back(CoherenceEvent {
            instruction,
            core,
            request,
            line: u64::from(line),
            transitions,
            supplied_by,
            flushed,
        });
    }
}
