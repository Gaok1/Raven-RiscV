
use super::*;
use crate::falcon::memory::Bus;

fn icfg_small() -> CacheConfig {
    CacheConfig {
        size: 64,
        line_size: 16,
        associativity: 1,
        replacement: ReplacementPolicy::Lru,
        write_policy: WritePolicy::WriteBack,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 10,
        assoc_penalty: 1,
        transfer_width: 8,
    }
}

fn dcfg(
    write_policy: WritePolicy,
    write_alloc: WriteAllocPolicy,
    size: usize,
    line_size: usize,
    assoc: usize,
) -> CacheConfig {
    CacheConfig {
        size,
        line_size,
        associativity: assoc,
        replacement: ReplacementPolicy::Lru,
        write_policy,
        write_alloc,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 10,
        assoc_penalty: 1,
        transfer_width: 8,
    }
}

// ── Caso 1: miss_pcs incrementa em misses de fetch ─────────────────────

#[test]
fn miss_pcs_increment_on_fetch_miss() {
    let mut ctrl = CacheController::new(icfg_small(), CacheConfig::default(), vec![], 256);

    // 1ª busca no addr 0 → cold miss; miss_pcs[0] deve ser 1
    ctrl.fetch32(0).unwrap();
    assert_eq!(
        *ctrl.icache.stats.miss_pcs.get(&0).unwrap_or(&0),
        1,
        "first fetch at 0 should record 1 miss"
    );

    // 2ª busca no mesmo addr 0 → hit (mesma linha); miss_pcs[0] não cresce
    ctrl.fetch32(0).unwrap();
    assert_eq!(
        *ctrl.icache.stats.miss_pcs.get(&0).unwrap_or(&0),
        1,
        "second fetch at 0 (hit) should not increment miss_pcs"
    );

    // busca no addr 16 → nova linha, cold miss; miss_pcs[16] == 1
    ctrl.fetch32(16).unwrap();
    assert_eq!(
        *ctrl.icache.stats.miss_pcs.get(&16).unwrap_or(&0),
        1,
        "fetch at addr 16 should record 1 miss"
    );
}

// ── Caso 2A: ram_write_bytes — write-through ────────────────────────────

#[test]
fn ram_write_bytes_write_through() {
    let d = dcfg(
        WritePolicy::WriteThrough,
        WriteAllocPolicy::WriteAllocate,
        64,
        16,
        1,
    );
    let mut ctrl = CacheController::new(CacheConfig::default(), d, vec![], 256);

    ctrl.store8(0, 42).unwrap();
    assert_eq!(
        ctrl.dcache.stats.ram_write_bytes, 1,
        "write-through store8 should write 1 byte to RAM immediately"
    );
    assert_eq!(ctrl.dcache.stats.bytes_stored, 1);
}

// ── Caso 2B: ram_write_bytes — write-back writeback on eviction ─────────

#[test]
fn ram_write_bytes_write_back_writeback() {
    // 1 set, 1 way, line_size=16 → eviction happens when switching between lines
    let d = dcfg(
        WritePolicy::WriteBack,
        WriteAllocPolicy::WriteAllocate,
        16,
        16,
        1,
    );
    let mut ctrl = CacheController::new(CacheConfig::default(), d, vec![], 256);

    // store8(0): miss → alloca linha 0, marca dirty; RAM NÃO é escrita ainda
    ctrl.store8(0, 1).unwrap();
    assert_eq!(
        ctrl.dcache.stats.ram_write_bytes, 0,
        "write-back miss should not write to RAM immediately"
    );

    // store8(16): miss → evict linha 0 (dirty) → writeback de 16 bytes para RAM
    ctrl.store8(16, 2).unwrap();
    assert_eq!(ctrl.dcache.stats.writebacks, 1);
    assert_eq!(
        ctrl.dcache.stats.ram_write_bytes, 16,
        "writeback should write exactly line_size bytes to RAM"
    );
}

#[test]
fn store_miss_write_allocate_uses_extra_level_latency() {
    let l1 = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        replacement: ReplacementPolicy::Lru,
        write_policy: WritePolicy::WriteBack,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
    };
    let l2 = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        replacement: ReplacementPolicy::Lru,
        write_policy: WritePolicy::WriteBack,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 5,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
    };
    let mut ctrl = CacheController::new(CacheConfig::default(), l1, vec![l2], 256);

    ctrl.store32(0x20, 0xDEAD_BEEF).unwrap();

    assert!(ctrl.extra_levels[0].stats.total_cycles > 0);
    assert_eq!(ctrl.load32(0x20).unwrap(), 0xDEAD_BEEF);
}

// ── Unaligned accesses should not panic ───────────────────────────────────

#[test]
fn unaligned_dcache_read16_across_line_does_not_panic() {
    let d = cfg(64, 16, 1);
    let mut ctrl = CacheController::new(CacheConfig::default(), d, vec![], 256);
    for i in 0u32..32 {
        ctrl.ram.store8(i, i as u8).unwrap();
    }

    // addr=15 reads bytes [15,16] across the 16-byte line boundary
    let v = ctrl.dcache_read16(15).unwrap();
    assert_eq!(v, u16::from_le_bytes([15, 16]));
}

#[test]
fn unaligned_dcache_read32_across_line_does_not_panic() {
    let d = cfg(64, 16, 1);
    let mut ctrl = CacheController::new(CacheConfig::default(), d, vec![], 256);
    for i in 0u32..32 {
        ctrl.ram.store8(i, i as u8).unwrap();
    }

    // addr=13 reads bytes [13,14,15,16] across the 16-byte line boundary
    let v = ctrl.dcache_read32(13).unwrap();
    assert_eq!(v, u32::from_le_bytes([13, 14, 15, 16]));
}

#[test]
fn unaligned_dcache_store32_across_line_does_not_panic() {
    let d = cfg(64, 16, 1);
    let mut ctrl = CacheController::new(CacheConfig::default(), d, vec![], 256);

    ctrl.store32(13, 0xAABB_CCDD).unwrap();
    assert_eq!(ctrl.effective_read32(13).unwrap(), 0xAABB_CCDD);
}

// ── Tipos de associatividade ──────────────────────────────────────────────

fn cfg(size: usize, line_size: usize, assoc: usize) -> CacheConfig {
    CacheConfig {
        size,
        line_size,
        associativity: assoc,
        replacement: ReplacementPolicy::Lru,
        write_policy: WritePolicy::WriteBack,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 10,
        assoc_penalty: 1,
        transfer_width: 8,
    }
}

// Mapeamento direto: assoc=1, cada endereço tem exatamente 1 posição na cache
#[test]
fn direct_mapped_is_valid_and_decomposes_correctly() {
    // size=1024, line=16, assoc=1 → 64 sets, 1 way each
    let c = cfg(1024, 16, 1);
    assert!(c.is_valid_config());
    assert_eq!(c.num_sets(), 64);
    assert_eq!(c.associativity, 1);
    assert_eq!(c.offset_bits(), 4); // 16 = 2^4
    assert_eq!(c.index_bits(), 6); // 64 = 2^6
    // tag = bits [31:10], index = bits [9:4], offset = bits [3:0]
    let addr = 0b_1010_1010__10_1010_10__1010u32;
    //                       ^ tag              ^ index  ^ offset
    assert_eq!(c.addr_offset(addr), (addr & 0xF) as usize);
    assert_eq!(c.addr_index(addr), ((addr >> 4) & 0x3F) as usize);
    assert_eq!(c.addr_tag(addr), addr >> 10);
}

// Mapeamento direto: conflito forçado → só 1 way, miss garantido ao acessar
// dois endereços que mapeiam para o mesmo set
#[test]
fn direct_mapped_conflict_causes_eviction() {
    // 1 set, 1 way, 16-byte line
    let c = cfg(16, 16, 1);
    assert!(c.is_valid_config());
    assert_eq!(c.num_sets(), 1); // degenerou em fully-assoc com 1 line — OK
    assert_eq!(c.associativity, 1);

    // Com size=64, line=16, assoc=1 → 4 sets, 1 way
    let c2 = cfg(64, 16, 1);
    assert!(c2.is_valid_config());
    assert_eq!(c2.num_sets(), 4);
    // addr=0x00 e addr=0x40 mapeiam para set 0 (índice = bits[5:4] = 00)
    assert_eq!(c2.addr_index(0x00), 0);
    assert_eq!(c2.addr_index(0x40), 0); // conflict: mesmo set, tag diferente
    assert_ne!(c2.addr_tag(0x00), c2.addr_tag(0x40));

    let mut ctrl = CacheController::new(CacheConfig::default(), c2.clone(), vec![], 256);
    ctrl.dcache_read8(0x00).unwrap(); // miss → instala tag 0x00 em set 0
    assert_eq!(ctrl.dcache.stats.misses, 1);
    ctrl.dcache_read8(0x00).unwrap(); // hit
    assert_eq!(ctrl.dcache.stats.hits, 1);
    ctrl.dcache_read8(0x40).unwrap(); // miss → conflito, evicta tag 0x00
    assert_eq!(ctrl.dcache.stats.misses, 2);
    assert_eq!(ctrl.dcache.stats.evictions, 1);
    ctrl.dcache_read8(0x00).unwrap(); // miss novamente (foi evictado)
    assert_eq!(ctrl.dcache.stats.misses, 3);
}

// Totalmente associativo: assoc = size/line_size → num_sets = 1
// Nenhum conflito possível (só capacidade e linha que limitam)
#[test]
fn fully_associative_no_conflict_misses() {
    // size=64, line=16, assoc=4 → 1 set, 4 ways
    let c = cfg(64, 16, 4);
    assert!(c.is_valid_config());
    assert_eq!(c.num_sets(), 1); // 1 único set
    assert_eq!(c.associativity, 4);
    assert_eq!(c.index_bits(), 0); // nenhum bit de índice
    assert_eq!(c.offset_bits(), 4);
    // Com 1 set: addr_index é sempre 0 para qualquer endereço
    assert_eq!(c.addr_index(0x000), 0);
    assert_eq!(c.addr_index(0x040), 0);
    assert_eq!(c.addr_index(0x080), 0);
    assert_eq!(c.addr_index(0xFFF), 0);

    let mut ctrl = CacheController::new(CacheConfig::default(), c, vec![], 256);
    // 4 acessos a linhas diferentes → 4 cold misses mas nenhum conflito
    ctrl.dcache_read8(0x00).unwrap(); // miss (way 0)
    ctrl.dcache_read8(0x10).unwrap(); // miss (way 1)
    ctrl.dcache_read8(0x20).unwrap(); // miss (way 2)
    ctrl.dcache_read8(0x30).unwrap(); // miss (way 3) — cache cheia
    assert_eq!(ctrl.dcache.stats.misses, 4);
    assert_eq!(ctrl.dcache.stats.evictions, 0); // nenhuma evicção ainda
    // Re-acessar qualquer um deles → hit (sem conflito)
    ctrl.dcache_read8(0x00).unwrap();
    ctrl.dcache_read8(0x10).unwrap();
    ctrl.dcache_read8(0x20).unwrap();
    ctrl.dcache_read8(0x30).unwrap();
    assert_eq!(ctrl.dcache.stats.hits, 4);
}

// Associativo por conjuntos: assoc>1 e sets>1 — comportamento intermediário
#[test]
fn set_associative_tolerates_limited_conflicts() {
    // size=128, line=16, assoc=2 → 4 sets, 2 ways cada
    let c = cfg(128, 16, 2);
    assert!(c.is_valid_config());
    assert_eq!(c.num_sets(), 4);
    assert_eq!(c.associativity, 2);
    assert_eq!(c.index_bits(), 2); // 4 = 2^2
    assert_eq!(c.offset_bits(), 4);

    // addr=0x00 e addr=0x40 mapeiam para o mesmo set (set 0)
    assert_eq!(c.addr_index(0x00), 0);
    assert_eq!(c.addr_index(0x40), 0);
    // Com 2 ways, os dois cabem SEM evicção
    let mut ctrl = CacheController::new(CacheConfig::default(), c, vec![], 512);
    ctrl.dcache_read8(0x00).unwrap(); // miss → way 0 do set 0
    ctrl.dcache_read8(0x40).unwrap(); // miss → way 1 do set 0 (ainda cabe)
    assert_eq!(ctrl.dcache.stats.misses, 2);
    assert_eq!(ctrl.dcache.stats.evictions, 0); // 2-way tolera 2 conflitos
    // Hit em ambos
    ctrl.dcache_read8(0x00).unwrap();
    ctrl.dcache_read8(0x40).unwrap();
    assert_eq!(ctrl.dcache.stats.hits, 2);
    // Terceiro endereço no mesmo set → evicção (cache 2-way está cheia)
    ctrl.dcache_read8(0x80).unwrap(); // miss → evicta LRU
    assert_eq!(ctrl.dcache.stats.evictions, 1);
}
