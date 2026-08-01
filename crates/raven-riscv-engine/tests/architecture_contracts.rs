use raven_riscv_engine::architectures::riscv32;
use raven_riscv_engine::capability::{CacheRole, RegisterId};
use raven_riscv_engine::falcon::memory::{Bus, Ram};
use raven_riscv_engine::{
    ArchitectureRegistry, Engine, MachineState, ProgramImage, ProgramSegment, StepOutcome, ZeroFill,
};

#[test]
fn builtin_registry_is_stable_and_runtime_selectable() {
    let registry = ArchitectureRegistry::with_builtins();
    let ids: Vec<_> = registry
        .architectures()
        .iter()
        .map(|a| a.descriptor().id)
        .collect();
    assert_eq!(ids, ["riscv32", "sap", "toy16"]);
    assert!(registry.get("missing").is_none());
    assert!(
        registry
            .get("riscv32")
            .unwrap()
            .descriptor()
            .capabilities
            .guided_learning
    );
    assert!(
        !registry
            .get("toy16")
            .unwrap()
            .descriptor()
            .capabilities
            .pipeline
    );
}

#[test]
fn every_builtin_can_assemble_load_and_halt_through_traits() {
    let registry = ArchitectureRegistry::with_builtins();
    for architecture in registry.architectures() {
        let engine = Engine::new(architecture.clone());
        let image = engine.assemble(architecture.default_source(), 0).unwrap();
        assert_eq!(image.architecture, architecture.descriptor().id);
        assert!(!image.executable_bytes().unwrap().is_empty());

        let mut machine = engine
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        machine.load(&image).unwrap();
        let outcome = machine.run(100).unwrap();
        assert!(matches!(
            outcome,
            StepOutcome::Halted | StepOutcome::Exited(_)
        ));
        assert!(matches!(
            machine.snapshot().state,
            MachineState::Halted | MachineState::Exited(_)
        ));
    }
}

#[test]
fn toy16_proves_dynamic_registers_memory_and_output() {
    let registry = ArchitectureRegistry::with_builtins();
    let engine = Engine::from_registry(&registry, "toy16").unwrap();
    let image = engine
        .assemble(
            "li r0, 20\nli r1, 22\nadd r2, r0, r1\nstore r2, [0x80]\nprint r2\nhalt",
            0,
        )
        .unwrap();
    let mut machine = engine.create_machine(64 * 1024).unwrap();
    machine.load(&image).unwrap();
    assert_eq!(machine.run(20).unwrap(), StepOutcome::Halted);
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.registers[2].value, 42);
    assert_eq!(snapshot.stdout, b"42\n");
    assert_eq!(machine.read_memory(0x80, 2).unwrap(), 42u16.to_le_bytes());
}

#[test]
fn the_builtin_registry_is_shared_and_cycles_through_every_backend() {
    let registry = ArchitectureRegistry::builtin();
    assert!(
        std::ptr::eq(registry, ArchitectureRegistry::builtin()),
        "builtin() must hand out one shared registry, not a fresh copy"
    );

    // Cycling has to visit every backend and come back to the start, so adding
    // an architecture never needs a UI change.
    let ids = registry.ids();
    let mut visited = vec![ids[0]];
    while visited.len() <= ids.len() {
        let next = registry.next_after(visited[visited.len() - 1]).unwrap();
        visited.push(next.descriptor().id);
    }
    assert_eq!(visited.first(), visited.last());
    let mut seen: Vec<_> = visited[..ids.len()].to_vec();
    seen.sort_unstable();
    assert_eq!(seen, ids);
}

#[test]
fn backends_reject_memory_they_cannot_address() {
    let registry = ArchitectureRegistry::builtin();
    for architecture in registry.architectures() {
        let descriptor = architecture.descriptor();
        let over = usize::try_from(descriptor.address_space_bytes())
            .ok()
            .and_then(|max| max.checked_add(1));
        let Some(over) = over else {
            continue; // address space is wider than usize on this host
        };
        assert!(
            architecture.create_machine(over).is_err(),
            "{} accepted more memory than it can address",
            descriptor.id
        );
        assert_eq!(
            descriptor.clamp_memory_size(over),
            usize::try_from(descriptor.address_space_bytes()).unwrap()
        );
        assert_eq!(
            descriptor.clamp_memory_size(0),
            descriptor.default_memory_size
        );
        architecture
            .create_machine(descriptor.default_memory_size)
            .unwrap_or_else(|e| panic!("{} rejected its own default: {e}", descriptor.id));
    }
}

#[test]
fn an_image_cannot_be_loaded_by_the_wrong_backend() {
    let registry = ArchitectureRegistry::with_builtins();
    let rv = Engine::from_registry(&registry, "riscv32").unwrap();
    let toy = Engine::from_registry(&registry, "toy16").unwrap();
    let image = rv.assemble("halt", 0).unwrap();
    let mut machine = toy.create_machine(64 * 1024).unwrap();
    assert!(
        machine
            .load(&image)
            .unwrap_err()
            .to_string()
            .contains("riscv32")
    );
}

#[test]
fn falc_v2_round_trips_backend_identity_and_segments() {
    let registry = ArchitectureRegistry::with_builtins();
    for id in ["riscv32", "sap", "toy16"] {
        let engine = Engine::from_registry(&registry, id).unwrap();
        let image = engine
            .assemble(engine.architecture().default_source(), 0)
            .unwrap();
        let decoded =
            raven_riscv_engine::ProgramImage::from_falc(&image.to_falc_v2().unwrap()).unwrap();
        assert_eq!(decoded.architecture, id);
        assert_eq!(decoded.entry, image.entry);
        assert_eq!(decoded.segments, image.segments);
        assert_eq!(decoded.zero_fill, image.zero_fill);
    }
}

#[test]
fn legacy_falc_v1_is_identified_as_riscv32() {
    let mut bytes = b"FALC".to_vec();
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0x0020_0073u32.to_le_bytes());
    let image = raven_riscv_engine::ProgramImage::from_falc(&bytes).unwrap();
    assert_eq!(image.architecture, "riscv32");
    assert_eq!(
        image.executable_bytes().unwrap(),
        &0x0020_0073u32.to_le_bytes()
    );
}

/// The shared installer is what makes the CLI, the TUI and the engine's own
/// machine agree on where a program lands. It must place every segment and
/// zero-fill exactly, and refuse an image that does not fit *before* writing
/// anything — a host that shows a load error must still have its old program.
#[test]
fn install_image_places_every_region_and_refuses_to_half_load() {
    let image = ProgramImage {
        architecture: riscv32::ID.into(),
        entry: 0,
        segments: vec![
            ProgramSegment {
                address: 0,
                bytes: vec![0xAA, 0xBB],
                executable: true,
                writable: false,
            },
            ProgramSegment {
                address: 0x40,
                bytes: vec![0xCC],
                executable: false,
                writable: true,
            },
        ],
        zero_fill: vec![ZeroFill {
            address: 0x41,
            size: 2,
        }],
        source_map: Default::default(),
    };

    let mut ram = Ram::new(0x100);
    ram.store8(0x41, 0xFF).unwrap();
    riscv32::install_image(&mut ram, &image).unwrap();
    assert_eq!(ram.load8(0).unwrap(), 0xAA);
    assert_eq!(ram.load8(0x40).unwrap(), 0xCC);
    assert_eq!(ram.load8(0x41).unwrap(), 0, "zero-fill did not run");
    // The heap starts on the first 16-byte boundary past everything loaded.
    assert_eq!(riscv32::heap_break_after(&image), 0x50);

    let mut small = Ram::new(0x10);
    assert!(riscv32::install_image(&mut small, &image).is_err());
    assert_eq!(
        small.load8(0).unwrap(),
        0,
        "a rejected image must not have written anything"
    );
}

/// A container the TUI opens and a container the CLI opens are the same
/// container: both go through `image_from_binary`, which also refuses images
/// built for another backend rather than running them as RV32.
#[test]
fn image_from_binary_reads_containers_and_rejects_foreign_ones() {
    let registry = ArchitectureRegistry::builtin();
    let rv = Engine::from_registry(registry, "riscv32").unwrap();
    let image = rv.assemble("li a0, 7\nhalt", 0).unwrap();
    let decoded = riscv32::image_from_binary(&image.to_falc_v2().unwrap(), 0).unwrap();
    assert_eq!(decoded.segments, image.segments);

    let toy = Engine::from_registry(registry, "toy16").unwrap();
    let foreign = toy
        .assemble(toy.architecture().default_source(), 0)
        .unwrap()
        .to_falc_v2()
        .unwrap();
    assert!(
        riscv32::image_from_binary(&foreign, 0)
            .unwrap_err()
            .to_string()
            .contains("toy16")
    );

    // Anything that is not a container is a flat block of code at `base`.
    let flat = riscv32::image_from_binary(&[0x73, 0x00, 0x20, 0x00], 0x1000).unwrap();
    assert_eq!(flat.entry, 0x1000);
    assert_eq!(flat.segments[0].address, 0x1000);
}

/// Loading is atomic: a machine that rejects an image keeps running the one it
/// already had, and a machine that accepts one has a usable heap break.
#[test]
fn a_rejected_load_leaves_the_previous_program_running() {
    let registry = ArchitectureRegistry::builtin();
    let rv = Engine::from_registry(registry, "riscv32").unwrap();
    let good = rv.assemble("li a0, 9\nhalt", 0).unwrap();
    let mut machine = rv.create_machine(64 * 1024).unwrap();
    machine.load(&good).unwrap();

    let mut oversized = good.clone();
    oversized.segments.push(ProgramSegment {
        address: 0xF000_0000,
        bytes: vec![0; 4],
        executable: false,
        writable: true,
    });
    assert!(machine.load(&oversized).is_err());

    let outcome = machine.run(20).unwrap();
    assert!(matches!(
        outcome,
        StepOutcome::Halted | StepOutcome::Exited(_)
    ));
    assert_eq!(machine.snapshot().registers[10].value, 9);
}

// ── Capability contracts ─────────────────────────────────────────────────────
//
// These run over every registered backend rather than over RV32, because that
// is the only way the surface stays ISA-neutral: a capability that quietly
// assumed 32 registers or four-byte instructions fails here on Toy16.

/// A declared register file has to be internally consistent: every id the banks
/// describe reads, names, and round-trips through `resolve`.
#[test]
fn every_declared_register_file_is_consistent() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(registers) = machine.registers() else {
            continue;
        };
        assert!(!registers.banks().is_empty(), "{id}: no register banks");

        let entries = registers.entries();
        let declared: usize = registers.banks().iter().map(|bank| bank.count).sum();
        assert_eq!(entries.len(), declared, "{id}: entries do not match banks");

        for entry in &entries {
            assert!(
                registers.read(entry.id).is_some(),
                "{id}: {} is described but does not read",
                entry.name
            );
            assert_eq!(
                registers.resolve(&entry.name),
                Some(entry.id),
                "{id}: {} does not resolve back to itself",
                entry.name
            );
            if let Some(alias) = &entry.alias {
                assert_eq!(
                    registers.resolve(alias),
                    Some(entry.id),
                    "{id}: alias {alias} does not resolve to {}",
                    entry.name
                );
            }
            assert!(
                entry.hex().len() * 4 >= usize::from(entry.bits),
                "{id}: {} prints too few hex digits for {} bits",
                entry.name,
                entry.bits
            );
        }
        // Ids outside the declared banks must be rejected, not wrapped.
        let past_end = RegisterId::new(0, registers.banks()[0].count);
        assert!(registers.read(past_end).is_none(), "{id}: read past bank end");
    }
}

/// Writes go where they say they go, and a refused write leaves the old value.
#[test]
fn every_declared_register_file_writes_where_it_says() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let mut machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(registers) = machine.registers_mut() else {
            continue;
        };
        for entry in registers.entries() {
            let before = registers.read(entry.id).unwrap();
            // A value that fits any bank width these backends declare.
            match registers.write(entry.id, 1) {
                Ok(()) => assert_eq!(
                    registers.read(entry.id),
                    Some(1),
                    "{id}: {} accepted a write it did not apply",
                    entry.name
                ),
                Err(_) => assert_eq!(
                    registers.read(entry.id),
                    Some(before),
                    "{id}: {} refused a write but changed anyway",
                    entry.name
                ),
            }
        }
    }
}

/// `peek` is the observer's read: it never fails, never runs past the end of
/// memory, and shows what `poke` just wrote.
#[test]
fn every_declared_memory_inspector_is_safe_at_the_edges() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let mut machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(memory) = machine.memory_mut() else {
            continue;
        };
        let size = memory.size();
        assert!(size > 0, "{id}: reports no memory");

        assert!(
            memory.peek(size, 16).is_empty(),
            "{id}: peek past the end returned data"
        );
        assert!(
            memory.peek(size - 4, 64).len() <= 4,
            "{id}: peek ran past the end of memory"
        );
        assert!(
            memory.peek(u64::MAX, 4).is_empty(),
            "{id}: peek at the top of the address space returned data"
        );

        memory.poke(8, &[0xAB, 0xCD]).unwrap();
        assert_eq!(memory.peek(8, 2), [0xAB, 0xCD], "{id}: peek did not see poke");
        assert_eq!(
            memory.peek_word(8, 2),
            0xCDAB,
            "{id}: peek_word is not little-endian over peek"
        );

        for region in memory.regions() {
            assert!(
                region.address < size,
                "{id}: region {} points outside memory",
                region.name
            );
        }
    }
}

/// A backend that can disassemble must be able to disassemble what its own
/// assembler produced, and must report a width that walks the listing forward.
#[test]
fn every_declared_codec_round_trips_its_own_default_program() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let engine = Engine::new(architecture.clone());
        let image = engine.assemble(architecture.default_source(), 0).unwrap();
        let mut machine = engine
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        machine.load(&image).unwrap();
        let (Some(code), Some(memory)) = (machine.code(), machine.memory()) else {
            continue;
        };

        let program_end = image.end_address();
        let mut address = image.entry;
        let mut decoded = 0;
        while address < program_end {
            let bytes = memory.peek(address, 8);
            let width = code.instruction_width(address, &bytes);
            assert!(width > 0, "{id}: zero instruction width would loop forever");
            if let Some(text) = code.disassemble(address, &bytes) {
                assert!(!text.trim().is_empty(), "{id}: empty disassembly");
                decoded += 1;
            }
            address += width as u64;
        }
        assert!(decoded > 0, "{id}: decoded nothing in its own default program");

        // What the codec assembles is what the machine would run.
        let encoded = code.assemble(image.entry, architecture.default_source()).unwrap();
        assert_eq!(
            encoded,
            image.executable_bytes().unwrap(),
            "{id}: codec and assembler disagree"
        );
    }
}

#[test]
fn cache_capability_matches_the_backend_descriptor() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        assert_eq!(
            machine.caches().is_some(),
            architecture.descriptor().capabilities.cache,
            "{}: cache descriptor and accessor disagree",
            architecture.descriptor().id
        );
    }
}

#[test]
fn every_declared_cache_hierarchy_has_consistent_levels_and_roles() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        assert!(caches.level_count() > 0, "{id}: empty cache hierarchy");
        assert!(caches.cache(0, CacheRole::Instruction).is_some());
        assert!(caches.cache(0, CacheRole::Data).is_some());
        assert!(caches.cache(0, CacheRole::Unified).is_none());

        let mut names = Vec::new();
        for level in 0..caches.level_count() {
            let roles: &[CacheRole] = if level == 0 {
                &[CacheRole::Instruction, CacheRole::Data]
            } else {
                &[CacheRole::Unified]
            };
            for &role in roles {
                let cache = caches.cache(level, role).unwrap();
                assert_eq!(cache.level, level, "{id}: wrong level index");
                assert_eq!(cache.role, role, "{id}: wrong cache role");
                assert!(!cache.name.trim().is_empty(), "{id}: unnamed cache");
                names.push(cache.name);
            }
        }
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            caches.level_count() + 1,
            "{id}: duplicate cache names"
        );
    }
}

#[test]
fn every_declared_cache_configuration_matches_its_contents() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let id = architecture.descriptor().id;
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        for level in 0..caches.level_count() {
            let roles: &[CacheRole] = if level == 0 {
                &[CacheRole::Instruction, CacheRole::Data]
            } else {
                &[CacheRole::Unified]
            };
            for &role in roles {
                let cache = caches.cache(level, role).unwrap();
                let cfg = cache.config;
                assert!(cfg.is_enabled(), "{id}: declared a disabled cache");
                assert_eq!(cfg.address_bits, architecture.descriptor().address_bits as usize);
                assert_eq!(
                    cfg.size,
                    cfg.num_sets * cfg.associativity * cfg.line_size,
                    "{id}: cache geometry does not equal its size"
                );
                for set_index in 0..cfg.num_sets {
                    let set = caches.set(level, role, set_index).unwrap();
                    assert_eq!(set.lines.len(), cfg.associativity);
                    assert!(set.lines.iter().all(|line| line.data.len() == cfg.line_size));
                }
            }
        }
    }
}

#[test]
fn every_cache_set_has_complete_replacement_metadata() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        for level in 0..caches.level_count() {
            let roles: &[CacheRole] = if level == 0 {
                &[CacheRole::Instruction, CacheRole::Data]
            } else {
                &[CacheRole::Unified]
            };
            for &role in roles {
                let cfg = caches.cache(level, role).unwrap().config;
                for set_index in 0..cfg.num_sets {
                    let set = caches.set(level, role, set_index).unwrap();
                    for order in [&set.lru_order, &set.fifo_order] {
                        let mut sorted = order.clone();
                        sorted.sort_unstable();
                        assert_eq!(sorted, (0..cfg.associativity).collect::<Vec<_>>());
                    }
                    assert!(set.clock_hand < cfg.associativity);
                }
            }
        }
    }
}

#[test]
fn fresh_cache_contents_and_statistics_are_empty() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        for role in [CacheRole::Instruction, CacheRole::Data] {
            let cache = caches.cache(0, role).unwrap();
            assert_eq!(cache.stats.total_accesses(), 0);
            assert_eq!(cache.stats.total_cycles, 0);
            assert!(cache.stats.history.is_empty());
            for set_index in 0..cache.config.num_sets {
                assert!(
                    caches
                        .set(0, role, set_index)
                        .unwrap()
                        .lines
                        .iter()
                        .all(|line| !line.valid && !line.dirty)
                );
            }
        }
    }
}

#[test]
fn cache_inspection_is_side_effect_free() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        let before: Vec<_> = [CacheRole::Instruction, CacheRole::Data]
            .into_iter()
            .map(|role| {
                let cache = caches.cache(0, role).unwrap();
                let sets: Vec<_> = (0..cache.config.num_sets)
                    .map(|set| {
                        let view = caches.set(0, role, set).unwrap();
                        (view.lru_order, view.fifo_order, view.clock_hand)
                    })
                    .collect();
                (cache.stats.hits, cache.stats.misses, cache.stats.total_cycles, sets)
            })
            .collect();
        let after: Vec<_> = [CacheRole::Instruction, CacheRole::Data]
            .into_iter()
            .map(|role| {
                let cache = caches.cache(0, role).unwrap();
                let sets: Vec<_> = (0..cache.config.num_sets)
                    .map(|set| {
                        let view = caches.set(0, role, set).unwrap();
                        (view.lru_order, view.fifo_order, view.clock_hand)
                    })
                    .collect();
                (cache.stats.hits, cache.stats.misses, cache.stats.total_cycles, sets)
            })
            .collect();
        assert_eq!(before, after);
    }
}

#[test]
fn executing_a_program_is_visible_through_the_cache_capability() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let engine = Engine::new(architecture.clone());
        let image = engine.assemble(architecture.default_source(), 0).unwrap();
        let mut machine = engine
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        if machine.caches().is_none() {
            continue;
        }
        machine.load(&image).unwrap();
        let _ = machine.step();
        let caches = machine.caches().unwrap();
        let instruction = caches.cache(0, CacheRole::Instruction).unwrap();
        assert!(instruction.stats.total_accesses() > 0);
        assert!(instruction.stats.total_cycles > 0);
        assert!(
            (0..instruction.config.num_sets).any(|set| caches
                .set(0, CacheRole::Instruction, set)
                .unwrap()
                .lines
                .iter()
                .any(|line| line.valid))
        );
    }
}

#[test]
fn cache_statistics_derived_values_are_consistent() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let engine = Engine::new(architecture.clone());
        let image = engine.assemble(architecture.default_source(), 0).unwrap();
        let mut machine = engine
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        if machine.caches().is_none() {
            continue;
        }
        machine.load(&image).unwrap();
        let _ = machine.step();
        let stats = machine
            .caches()
            .unwrap()
            .cache(0, CacheRole::Instruction)
            .unwrap()
            .stats;
        assert_eq!(stats.total_accesses(), stats.hits + stats.misses);
        assert!((0.0..=100.0).contains(&stats.hit_rate()));
        assert_eq!(stats.mpki(0), 0.0);
        assert_eq!(
            stats.mpki(1),
            stats.misses as f64 * 1000.0,
            "MPKI must use the exposed miss counter"
        );
    }
}

#[test]
fn cache_capability_checks_every_index() {
    for architecture in ArchitectureRegistry::builtin().architectures() {
        let machine = architecture
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        let Some(caches) = machine.caches() else {
            continue;
        };
        assert!(caches.cache(0, CacheRole::Unified).is_none());
        assert!(caches.cache(0, CacheRole::Instruction).is_some());
        assert!(
            caches
                .cache(caches.level_count(), CacheRole::Unified)
                .is_none()
        );
        assert!(
            caches
                .set(0, CacheRole::Instruction, usize::MAX)
                .is_none()
        );
        if caches.level_count() > 1 {
            assert!(caches.cache(1, CacheRole::Instruction).is_none());
            assert!(caches.cache(1, CacheRole::Data).is_none());
        }
    }
}

#[test]
fn falc_v2_rejects_unrepresentable_metadata_and_truncation() {
    let mut image = ProgramImage {
        architecture: "x".repeat(usize::from(u16::MAX) + 1),
        entry: 0,
        segments: vec![],
        zero_fill: vec![],
        source_map: Default::default(),
    };
    assert!(image.to_falc_v2().is_err());

    image.architecture = "toy16".into();
    image.segments.push(ProgramSegment {
        address: 0,
        bytes: vec![0, 0],
        executable: true,
        writable: false,
    });
    let mut bytes = image.to_falc_v2().unwrap();
    bytes.pop();
    assert!(ProgramImage::from_falc(&bytes).is_err());
}
