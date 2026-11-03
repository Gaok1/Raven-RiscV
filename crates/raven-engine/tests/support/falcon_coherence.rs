// Coherence between the cores' private L1 caches.

use super::*;
use crate::capability::{CacheHierarchy, CacheRole, CoherenceRequest, CoherenceState};
use crate::falcon::memory::{AmoOp, Bus};

const X: u32 = 0x1000;
const Y: u32 = 0x2000;

fn l1(write_policy: WritePolicy) -> CacheConfig {
    CacheConfig {
        size: 256,
        line_size: 16,
        associativity: 2,
        replacement: ReplacementPolicy::Lru,
        write_policy,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 10,
        assoc_penalty: 1,
        transfer_width: 8,
    }
}

/// Two cores with private write-back L1s under `protocol`, no L2.
fn machine(protocol: CoherenceProtocol) -> CacheController {
    machine_with(protocol, WritePolicy::WriteBack)
}

fn machine_with(protocol: CoherenceProtocol, write_policy: WritePolicy) -> CacheController {
    let mut mem = CacheController::new(l1(write_policy), l1(write_policy), vec![], 64 * 1024);
    mem.set_coherence(CoherenceConfig {
        protocol,
        ..CoherenceConfig::default()
    });
    mem.set_core_count(2);
    mem
}

fn on(mem: &mut CacheController, core: usize) -> &mut CacheController {
    mem.set_active_core(core);
    mem
}

/// `core`'s D-cache state for `addr`'s line.
fn state(mem: &CacheController, core: usize, addr: u32) -> CoherenceState {
    let view = mem.core_cache(core, 0, CacheRole::Data).unwrap();
    let cfg = view.config;
    let set = ((addr >> cfg.offset_bits()) as usize) & (cfg.num_sets() - 1);
    let tag = u64::from(addr >> (cfg.offset_bits() + cfg.index_bits()));
    mem.core_set(core, 0, CacheRole::Data, set)
        .unwrap()
        .lines
        .iter()
        .find(|line| line.valid && line.tag == tag)
        .map_or(CoherenceState::Invalid, |line| line.state.unwrap())
}

fn counters(mem: &CacheController, core: usize) -> crate::capability::CoherenceCoreStats {
    mem.coherence_tracker().cores[core]
}

#[test]
fn shared_l1_is_the_default_and_has_nothing_to_keep_coherent() {
    let mem = CacheController::new(
        l1(WritePolicy::WriteBack),
        l1(WritePolicy::WriteBack),
        vec![],
        4096,
    );
    assert_eq!(mem.coherence_protocol(), CoherenceProtocol::Shared);
    assert_eq!(mem.core_count(), 1);
    assert!(mem.coherence().is_none());
}

#[test]
fn shared_l1_lets_every_core_see_every_store() {
    let mut mem = machine(CoherenceProtocol::Shared);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 1).store32(X, 7).unwrap();
    assert_eq!(on(&mut mem, 0).dcache_read32(X).unwrap(), 7);
    assert_eq!(mem.core_count(), 1);
}

#[test]
fn without_a_protocol_a_core_keeps_reading_its_stale_copy() {
    let mut mem = machine(CoherenceProtocol::None);
    assert_eq!(on(&mut mem, 0).dcache_read32(X).unwrap(), 0);
    on(&mut mem, 1).store32(X, 7).unwrap();
    // Core 1 sees its own write; core 0 still has the line it read before.
    assert_eq!(on(&mut mem, 1).dcache_read32(X).unwrap(), 7);
    assert_eq!(on(&mut mem, 0).dcache_read32(X).unwrap(), 0);
    // Nothing went on a bus.
    assert_eq!(counters(&mem, 1).transactions(), 0);
}

#[test]
fn msi_reads_share_writes_invalidate_and_modified_lines_flush() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Msi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    // MSI has no Exclusive: a lone reader is Shared.
    assert_eq!(state(&mem, 0, X), Shared);
    on(&mut mem, 1).dcache_read32(X).unwrap();
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Shared, Shared));

    on(&mut mem, 0).store32(X, 7).unwrap();
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Modified, Invalid));
    assert_eq!(counters(&mem, 0).bus_upgrades, 1);
    assert_eq!(counters(&mem, 1).invalidations_received, 1);

    assert_eq!(on(&mut mem, 1).dcache_read32(X).unwrap(), 7);
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Shared, Shared));
    assert_eq!(counters(&mem, 0).flushes, 1);
    assert_eq!(counters(&mem, 1).coherence_misses, 1);
    // The flush reached memory.
    assert_eq!(mem.peek32(X).unwrap(), 7);
}

#[test]
fn msi_upgrades_even_a_line_no_other_core_holds() {
    let mut mem = machine(CoherenceProtocol::Msi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 0).store32(X, 1).unwrap();
    assert_eq!(counters(&mem, 0).bus_upgrades, 1);
}

#[test]
fn mesi_writes_an_unshared_line_without_a_bus_request() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    assert_eq!(state(&mem, 0, X), Exclusive);
    let before = counters(&mem, 0).transactions();
    on(&mut mem, 0).store32(X, 5).unwrap();
    assert_eq!(state(&mem, 0, X), Modified);
    assert_eq!(counters(&mem, 0).transactions(), before);

    // A second reader: the Modified copy is written back and both share.
    assert_eq!(on(&mut mem, 1).dcache_read32(X).unwrap(), 5);
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Shared, Shared));
    assert_eq!(counters(&mem, 0).flushes, 1);
}

#[test]
fn mesi_a_line_read_while_another_core_holds_it_is_shared() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 1).dcache_read32(X).unwrap();
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Shared, Shared));
}

#[test]
fn moesi_shares_a_dirty_line_and_sends_it_cache_to_cache() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Moesi);
    on(&mut mem, 0).store32(X, 9).unwrap();
    assert_eq!(state(&mem, 0, X), Modified);

    assert_eq!(on(&mut mem, 1).dcache_read32(X).unwrap(), 9);
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Owned, Shared));
    assert_eq!(counters(&mem, 0).cache_to_cache, 1);
    assert_eq!(counters(&mem, 0).flushes, 0);
    // Memory was not written: the owner still answers for the line.
    assert_eq!(mem.peek32(X).unwrap(), 0);
    let event = mem.coherence_tracker().events.back().unwrap();
    assert_eq!(event.request, CoherenceRequest::BusRd);
    assert_eq!(event.supplied_by, Some(0));

    // Core 1 writes: the Owned copy goes, core 1 is the new Modified owner.
    on(&mut mem, 1).store32(X, 10).unwrap();
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Invalid, Modified));
    assert_eq!(on(&mut mem, 0).dcache_read32(X).unwrap(), 10);
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Shared, Owned));
}

#[test]
fn a_write_miss_reads_for_ownership() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 1).store32(X, 3).unwrap();
    assert_eq!(counters(&mem, 1).bus_read_exclusive, 1);
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Invalid, Modified));
}

#[test]
fn an_amo_asks_for_ownership_once() {
    use CoherenceState::*;
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 1).dcache_read32(X).unwrap();
    let old = on(&mut mem, 0).amo_w(0, X, AmoOp::Add, 4).unwrap();
    assert_eq!(old, 0);
    let c = counters(&mem, 0);
    assert_eq!(
        (c.bus_read_exclusive, c.bus_reads, c.bus_upgrades),
        (1, 0, 0)
    );
    assert_eq!((state(&mem, 0, X), state(&mem, 1, X)), (Modified, Invalid));
    assert_eq!(on(&mut mem, 1).dcache_read32(X).unwrap(), 4);
}

#[test]
fn write_through_stores_go_on_the_bus_and_invalidate_other_copies() {
    let mut mem = machine_with(CoherenceProtocol::Msi, WritePolicy::WriteThrough);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 1).dcache_read32(X).unwrap();
    on(&mut mem, 1).store32(X, 6).unwrap();
    assert_eq!(counters(&mem, 1).bus_writes, 1);
    assert_eq!(state(&mem, 0, X), CoherenceState::Invalid);
    assert_eq!(on(&mut mem, 0).dcache_read32(X).unwrap(), 6);
}

#[test]
fn an_instruction_fetch_sees_code_another_core_left_dirty() {
    let mut mem = machine(CoherenceProtocol::Mesi);
    // `addi x0, x0, 0` written by core 1 sits Modified in its D-cache.
    on(&mut mem, 1).store32(Y, 0x0000_0013).unwrap();
    assert_eq!(mem.peek32(Y).unwrap(), 0);
    assert_eq!(on(&mut mem, 0).fetch32(Y).unwrap(), 0x0000_0013);
    assert_eq!(state(&mem, 1, Y), CoherenceState::Shared);
}

#[test]
fn effective_reads_find_the_dirty_copy_another_core_owns() {
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 1).store32(X, 11).unwrap();
    mem.set_active_core(0);
    assert_eq!(mem.load32(X).unwrap(), 11);

    let mut stale = machine(CoherenceProtocol::None);
    on(&mut stale, 1).store32(X, 11).unwrap();
    stale.set_active_core(0);
    // Without a protocol core 0 does not see core 1's private copy.
    assert_eq!(stale.load32(X).unwrap(), 0);
}

#[test]
fn sync_to_ram_writes_back_every_core_s_dirty_lines() {
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 0).store32(X, 1).unwrap();
    on(&mut mem, 1).store32(Y, 2).unwrap();
    mem.sync_to_ram();
    assert_eq!((mem.peek32(X).unwrap(), mem.peek32(Y).unwrap()), (1, 2));
}

#[test]
fn switching_cores_keeps_each_core_s_caches_and_counters() {
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 0).dcache_read32(X).unwrap();
    on(&mut mem, 1).dcache_read32(Y).unwrap();
    for _ in 0..3 {
        mem.set_active_core(0);
        mem.set_active_core(1);
    }
    assert_eq!(state(&mem, 0, X), CoherenceState::Exclusive);
    assert_eq!(state(&mem, 1, Y), CoherenceState::Exclusive);
    assert_eq!(state(&mem, 0, Y), CoherenceState::Invalid);
    let misses = |core| {
        mem.core_cache(core, 0, CacheRole::Data)
            .unwrap()
            .stats
            .misses
    };
    assert_eq!((misses(0), misses(1)), (1, 1));
}

#[test]
fn a_snapshot_restores_every_core_s_caches() {
    let mut mem = machine(CoherenceProtocol::Moesi);
    on(&mut mem, 0).store32(X, 1).unwrap();
    let snap = mem.snapshot_state();
    on(&mut mem, 1).dcache_read32(X).unwrap();
    assert_eq!(state(&mem, 0, X), CoherenceState::Owned);
    mem.restore_state(snap);
    assert_eq!(mem.active_core(), 0);
    assert_eq!(state(&mem, 0, X), CoherenceState::Modified);
    assert_eq!(state(&mem, 1, X), CoherenceState::Invalid);
}

#[test]
fn leaving_a_private_protocol_keeps_dirty_data() {
    let mut mem = machine(CoherenceProtocol::Mesi);
    on(&mut mem, 1).store32(X, 8).unwrap();
    mem.set_active_core(0);
    mem.set_coherence(CoherenceConfig::default());
    assert_eq!(mem.core_count(), 1);
    assert_eq!(mem.load32(X).unwrap(), 8);
}

#[test]
fn the_bus_charges_its_latencies() {
    let mut mem = machine(CoherenceProtocol::Moesi);
    mem.set_coherence_latencies(7, 3);
    on(&mut mem, 0).store32(X, 1).unwrap();
    let before = mem.total_program_cycles();
    on(&mut mem, 1).dcache_read32(X).unwrap();
    // tag search (1 + 1 way penalty) + snoop 7 + cache-to-cache 3.
    assert_eq!(mem.total_program_cycles() - before, 2 + 7 + 3);
    assert_eq!(counters(&mem, 1).bus_cycles, 7 + 3);
}

#[test]
fn coherence_config_round_trips_through_rcfg() {
    let config = CoherenceConfig {
        protocol: CoherenceProtocol::Moesi,
        snoop_latency: 3,
        cache_to_cache_latency: 12,
    };
    assert_eq!(
        CoherenceConfig::from_rcfg(&config.to_rcfg()).unwrap(),
        config
    );
    // A file from before coherence existed loads as one shared L1.
    assert_eq!(
        CoherenceConfig::from_rcfg("icache.size=64\n").unwrap(),
        CoherenceConfig::default()
    );
    assert!(CoherenceConfig::from_rcfg("coherence=dragon\n").is_err());
    for protocol in CoherenceProtocol::ALL {
        assert_eq!(
            CoherenceProtocol::from_str(protocol.as_str()),
            Some(protocol)
        );
    }
}
