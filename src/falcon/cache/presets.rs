// falcon/cache/presets.rs — Standard preset configurations for L1 and L2+ caches

use super::{CacheConfig, InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy};

/// Returns [Small, Medium, Large] preset configs for I-cache or D-cache.
pub fn cache_presets(icache: bool) -> [CacheConfig; 3] {
    if icache {
        [
            CacheConfig {
                size: 256,
                line_size: 16,
                associativity: 1,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 1024,
                line_size: 16,
                associativity: 2,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 4096,
                line_size: 32,
                associativity: 4,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
        ]
    } else {
        [
            CacheConfig {
                size: 256,
                line_size: 16,
                associativity: 1,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 1024,
                line_size: 16,
                associativity: 2,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 8192,
                line_size: 32,
                associativity: 4,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
        ]
    }
}

/// Returns [Small, Medium, Large] preset configs for a unified L2/L3 cache.
pub fn extra_level_presets() -> [CacheConfig; 3] {
    [
        CacheConfig {
            size: 8192,
            line_size: 64,
            associativity: 4,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 5,
            miss_penalty: 200,
            assoc_penalty: 1,
            transfer_width: 8,
        },
        CacheConfig {
            size: 65536,
            line_size: 64,
            associativity: 8,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 10,
            miss_penalty: 400,
            assoc_penalty: 1,
            transfer_width: 8,
        },
        CacheConfig {
            size: 524288,
            line_size: 128,
            associativity: 16,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 20,
            miss_penalty: 600,
            assoc_penalty: 1,
            transfer_width: 8,
        },
    ]
}
