use super::*;

fn cfg(entries: u16, assoc: u8, policy: ReplacementPolicy) -> TlbConfig {
    TlbConfig {
        entry_count: entries,
        associativity: assoc,
        replacement: policy,
        hit_latency: 1,
        miss_penalty: 20,
    }
}

fn mk_entry(vpn: u32, ppn: u32, asid: u16) -> TlbEntry {
    TlbEntry {
        valid: true,
        vpn,
        ppn,
        asid,
        perms: PtePerms {
            r: true,
            w: true,
            x: true,
            u: true,
        },
        global: false,
        accessed: true,
        dirty: false,
        mask_bits: 0,
        age: 0,
        ref_bit: false,
    }
}

#[test]
fn install_then_probe_hits() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    tlb.install(mk_entry(0x10, 0x100, 1));
    let e = tlb.probe(0x10, 1).expect("hit");
    assert_eq!(e.ppn, 0x100);
}

#[test]
fn probe_miss_on_wrong_asid() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    tlb.install(mk_entry(0x10, 0x100, 1));
    assert!(tlb.probe(0x10, 2).is_none());
}

#[test]
fn global_entry_matches_any_asid() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    let mut e = mk_entry(0x10, 0x100, 1);
    e.global = true;
    tlb.install(e);
    assert!(tlb.probe(0x10, 2).is_some());
    assert!(tlb.probe(0x10, 99).is_some());
}

#[test]
fn megapage_matches_any_vpn0() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    let mut e = mk_entry(0x4000, 0x4000, 1); // vpn1=16, vpn0=0
    e.mask_bits = 10;
    tlb.install(e);
    // Different vpn0 within same vpn1 must hit.
    assert!(tlb.probe(0x4000 | 0x123, 1).is_some());
    // Different vpn1 must miss.
    assert!(tlb.probe(0x8000, 1).is_none());
}

#[test]
fn flush_invalidates_all() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    tlb.install(mk_entry(0x10, 0x100, 1));
    tlb.install(mk_entry(0x11, 0x101, 1));
    tlb.flush();
    assert!(tlb.probe(0x10, 1).is_none());
    assert!(tlb.probe(0x11, 1).is_none());
}

#[test]
fn flush_vaddr_targets_one_entry() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    tlb.install(mk_entry(0x10, 0x100, 1));
    tlb.install(mk_entry(0x11, 0x101, 1));
    tlb.flush_vaddr(0x10 << 12);
    assert!(tlb.probe(0x10, 1).is_none());
    assert!(tlb.probe(0x11, 1).is_some());
}

#[test]
fn lru_evicts_least_recently_used() {
    // 2 entries, fully-associative â†’ 1 set, 2 ways.
    let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Lru));
    // All three VPNs hash to set 0 (only 1 set).
    tlb.install(mk_entry(0x10, 0xA, 1));
    tlb.install(mk_entry(0x11, 0xB, 1));
    // Touch 0x10 so 0x11 becomes the LRU.
    tlb.probe(0x10, 1).unwrap();
    tlb.install(mk_entry(0x12, 0xC, 1));
    // 0x11 should have been evicted.
    assert!(tlb.probe(0x11, 1).is_none());
    assert!(tlb.probe(0x10, 1).is_some());
    assert!(tlb.probe(0x12, 1).is_some());
    assert_eq!(tlb.stats.evictions, 1);
}

#[test]
fn fifo_evicts_oldest_install_regardless_of_touch() {
    let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Fifo));
    tlb.install(mk_entry(0x10, 0xA, 1));
    tlb.install(mk_entry(0x11, 0xB, 1));
    // Touching 0x10 in FIFO must NOT save it from eviction.
    tlb.probe(0x10, 1).unwrap();
    tlb.install(mk_entry(0x12, 0xC, 1));
    assert!(tlb.probe(0x10, 1).is_none(), "0x10 should be evicted");
    assert!(tlb.probe(0x11, 1).is_some());
    assert!(tlb.probe(0x12, 1).is_some());
}
