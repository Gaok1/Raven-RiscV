// Phase 4 verification: the cache-controller `*_timed` API must surface the
// TLB miss penalty in its returned latency so the pipeline simulator can stall
// `if_stall_cycles` / `mem_stall_cycles` for the right number of cycles.

use raven::falcon::cache::{CacheConfig, CacheController};
use raven::falcon::memory::Bus;
use raven::falcon::mmu::{PrivMode, TlbConfig};
use raven::falcon::cache::ReplacementPolicy;

const RAM_SIZE: usize = 1 << 20;

fn install_one_page(mem: &mut CacheController, vaddr: u32, paddr: u32, perms_bits: u32) -> u32 {
    let root_pt_pa: u32 = 0x1000;
    let leaf_pt_pa: u32 = 0x2000;
    let root_ppn = root_pt_pa >> 12;
    let leaf_ppn = leaf_pt_pa >> 12;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let vpn0 = (vaddr >> 12) & 0x3FF;
    let pte1 = (leaf_ppn << 10) | 0x1;
    mem.ram_mut().store32(root_pt_pa + vpn1 * 4, pte1).unwrap();
    let ppn = paddr >> 12;
    let pte0 = (ppn << 10) | perms_bits | 0x1;
    mem.ram_mut().store32(leaf_pt_pa + vpn0 * 4, pte0).unwrap();
    (1u32 << 31) | root_ppn
}

fn build_controller(tlb_cfg: TlbConfig) -> CacheController {
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        RAM_SIZE,
    );
    mem.mmu_mut().tlb.reconfigure(tlb_cfg);
    mem.mmu_mut().enabled = true;
    mem.mmu_mut().priv_mode = PrivMode::U;
    mem
}

fn small_tlb(miss_penalty: u8, hit_latency: u8) -> TlbConfig {
    TlbConfig {
        entry_count: 2,
        associativity: 1,
        replacement: ReplacementPolicy::Lru,
        hit_latency,
        miss_penalty,
    }
}

#[test]
fn dcache_read_timed_includes_tlb_miss_penalty() {
    let mut mem = build_controller(small_tlb(30, 1));
    let satp = install_one_page(&mut mem, 0x10_0000, 0x8_0000, 0x2 | 0x4 | 0x10);
    mem.set_satp(satp);

    // Store a value via the bus so the page is mapped + populated.
    let (store_res, _) = mem.store32_timed(0x10_0000, 0xDEAD_BEEF);
    store_res.unwrap();

    // Reset TLB stats so we measure only the next two reads.
    mem.mmu_mut().tlb.stats.hits = 0;
    mem.mmu_mut().tlb.stats.misses = 0;

    // Flush the TLB so the next access misses for sure.
    mem.tlb_flush();

    let (miss_res, miss_latency) = mem.dcache_read32_timed(0x10_0000);
    assert_eq!(miss_res.unwrap(), 0xDEAD_BEEF);
    let (hit_res, hit_latency) = mem.dcache_read32_timed(0x10_0000);
    assert_eq!(hit_res.unwrap(), 0xDEAD_BEEF);

    assert_eq!(mem.mmu().tlb.stats.misses, 1, "first access is a TLB miss");
    assert_eq!(mem.mmu().tlb.stats.hits, 1, "second access is a TLB hit");

    // The TLB miss penalty is 30, hit latency 1. The miss-path latency must be
    // at least 30 cycles greater than the cache-only baseline; the hit-path
    // latency must include the 1-cycle TLB hit on top of the cache hit.
    assert!(
        miss_latency >= 30,
        "miss latency ({}) should include the 30-cycle TLB miss penalty",
        miss_latency
    );
    assert!(
        miss_latency > hit_latency,
        "TLB miss path ({}) must be slower than TLB hit path ({})",
        miss_latency,
        hit_latency
    );
}

#[test]
fn fetch32_timed_includes_tlb_miss_penalty() {
    let mut mem = build_controller(small_tlb(25, 1));
    // Map VA 0 → PA 0 with RX+U so fetch can succeed under VM.
    let satp = install_one_page(&mut mem, 0, 0, 0x2 | 0x8 | 0x10);
    mem.set_satp(satp);

    // Write a NOP-ish word at PA 0.
    mem.ram_mut().store32(0, 0x00000013).unwrap();

    // Warm-load the page, then flush TLB so we get a guaranteed miss.
    let _ = mem.fetch32_timed(0);
    mem.tlb_flush();
    mem.mmu_mut().tlb.stats.misses = 0;
    mem.mmu_mut().tlb.stats.hits = 0;

    let (_word, miss_latency) = mem.fetch32_timed(0);
    assert_eq!(mem.mmu().tlb.stats.misses, 1);
    assert!(
        miss_latency >= 25,
        "fetch miss latency ({}) should include the 25-cycle TLB miss penalty",
        miss_latency
    );
}

#[test]
fn alternating_two_pages_force_repeated_tlb_misses() {
    // One-entry direct-mapped TLB → two VPNs that collide on the same set
    // alternate-evict each other, so every other access is a miss.
    let mut mem = build_controller(small_tlb(10, 1));
    let satp_a = install_one_page(&mut mem, 0x10_0000, 0x8_0000, 0x2 | 0x4 | 0x10);
    // VA 0x10_1000 shares vpn[1]=0 with 0x10_0000 → also goes through the same
    // leaf table that install_one_page just wrote. Add the second leaf entry.
    let vpn0_b = (0x10_1000u32 >> 12) & 0x3FF;
    let pte0_b = ((0x9_0000u32 >> 12) << 10) | 0x2 | 0x4 | 0x10 | 0x1;
    mem.ram_mut()
        .store32(0x2000 + vpn0_b * 4, pte0_b)
        .unwrap();
    mem.set_satp(satp_a);

    // Both pages must hash to the same TLB set. With entry_count=2, assoc=1
    // → 2 sets; `vpn >> 10` is what indexes a set, and 0x10_0000 and 0x10_1000
    // differ only in vpn[0]=0 vs 1, so vpn >> 10 is identical → same set,
    // perfect collision.
    mem.tlb_flush();
    mem.mmu_mut().tlb.stats.hits = 0;
    mem.mmu_mut().tlb.stats.misses = 0;

    let addrs = [0x10_0000u32, 0x10_1000];
    let mut total_latency = 0u64;
    for round in 0..8 {
        let (_r, lat) = mem.dcache_read32_timed(addrs[round & 1]);
        total_latency += lat;
    }

    // With entry_count=2, assoc=1, and both pages in the same set, the second
    // entry kicks the first out; every access is a miss.
    assert!(
        mem.mmu().tlb.stats.misses >= 8,
        "expected at least 8 TLB misses, got {} hits / {} misses",
        mem.mmu().tlb.stats.hits,
        mem.mmu().tlb.stats.misses
    );
    assert!(
        total_latency >= 8 * 10,
        "8 TLB misses × 10-cycle penalty = 80, got total latency {}",
        total_latency
    );
}
