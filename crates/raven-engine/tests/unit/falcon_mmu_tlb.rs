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
        dirty: false,
        megapage: false,
        age: 0,
        ref_bit: false,
    }
}

#[test]
fn install_then_probe_hits() {
    let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
    tlb.install(mk_entry(0x10, 0x100, 1));
    let (_, e) = tlb.probe(0x10, 1).expect("hit");
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
    e.megapage = true;
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

#[test]
fn megapage_is_filed_under_its_top_vpn_bits() {
    // 8 sets of 1 way. A megapage at VPN[1]=3 lives in set 3, whatever VPN[0]
    // the address that brought it in had.
    let mut tlb = Tlb::new(cfg(8, 1, ReplacementPolicy::Lru));
    let mut e = mk_entry(3 << 10, 3 << 10, 1);
    e.megapage = true;
    let installed = tlb.install(e);
    assert_eq!(installed.slot, 3);
    assert_eq!(tlb.probe_sets((3 << 10) | 0x2A5), [5, 3]);
    assert!(tlb.probe((3 << 10) | 0x2A5, 1).is_some());
}

#[test]
fn install_reports_the_entry_it_evicted() {
    let mut tlb = Tlb::new(cfg(1, 1, ReplacementPolicy::Lru));
    assert_eq!(tlb.install(mk_entry(0x10, 0xA, 1)).evicted, None);
    let second = tlb.install(mk_entry(0x11, 0xB, 1));
    assert_eq!(second.slot, 0);
    assert_eq!(second.evicted.map(|e| e.vpn), Some(0x10));
}

#[test]
fn recency_ranks_entries_within_their_set() {
    let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Lru));
    let a = tlb.install(mk_entry(0x10, 0xA, 1)).slot;
    let b = tlb.install(mk_entry(0x11, 0xB, 1)).slot;
    assert_eq!(tlb.recency(b), Some(1));
    assert_eq!(tlb.recency(a), Some(2));
    tlb.probe(0x10, 1).unwrap();
    assert_eq!(tlb.recency(a), Some(1));
    assert_eq!(tlb.recency(b), Some(2));
}

#[test]
fn find_has_no_side_effects() {
    let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Lru));
    let a = tlb.install(mk_entry(0x10, 0xA, 1)).slot;
    let b = tlb.install(mk_entry(0x11, 0xB, 1)).slot;
    assert_eq!(tlb.find(0x10, 1), Some(a));
    // Finding 0x10 must not make it the most recent.
    assert_eq!(tlb.recency(b), Some(1));
}
