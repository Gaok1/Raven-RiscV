use raven_engine::architectures::riscv32;
use raven_engine::capability::{
    CacheRole, InstructionCodec, PipelineEdge, PipelineEdgeKind, PipelineInspect,
    PipelineStageRole, PipelineStageView, PipelineStats, PipelineStatus, PipelineTimelineCell,
    PipelineTimelineRow, PipelineTraceView, PipelineUnitView, RegisterId, TranslationOutcome,
};
use raven_engine::falcon::memory::{Bus, Ram};
use raven_engine::{
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
    assert_eq!(ids, ["riscv32", "sap", "toy16", "x86_64"]);
    assert!(registry.get("missing").is_none());
    assert!(
        registry
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
        let snapshot = machine.snapshot();
        assert!(matches!(
            snapshot.state,
            MachineState::Halted | MachineState::Exited(_)
        ));
        assert_eq!(snapshot.stdout, b"Hello, World!\n");
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
            raven_engine::ProgramImage::from_falc(&image.to_falc_v2().unwrap()).unwrap();
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
    let image = raven_engine::ProgramImage::from_falc(&bytes).unwrap();
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
        assert!(
            registers.read(past_end).is_none(),
            "{id}: read past bank end"
        );
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
        assert_eq!(
            memory.peek(8, 2),
            [0xAB, 0xCD],
            "{id}: peek did not see poke"
        );
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
        assert!(
            decoded > 0,
            "{id}: decoded nothing in its own default program"
        );

        // What the codec assembles is what the machine would run.
        let encoded = code
            .assemble(image.entry, architecture.default_source())
            .unwrap();
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
                assert_eq!(
                    cfg.address_bits,
                    architecture.descriptor().address_bits as usize
                );
                assert_eq!(
                    cfg.size,
                    cfg.num_sets * cfg.associativity * cfg.line_size,
                    "{id}: cache geometry does not equal its size"
                );
                for set_index in 0..cfg.num_sets {
                    let set = caches.set(level, role, set_index).unwrap();
                    assert_eq!(set.lines.len(), cfg.associativity);
                    assert!(
                        set.lines
                            .iter()
                            .all(|line| line.data.len() == cfg.line_size)
                    );
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
                (
                    cache.stats.hits,
                    cache.stats.misses,
                    cache.stats.total_cycles,
                    sets,
                )
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
                (
                    cache.stats.hits,
                    cache.stats.misses,
                    cache.stats.total_cycles,
                    sets,
                )
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
        assert!((0..instruction.config.num_sets).any(|set| {
            caches
                .set(0, CacheRole::Instruction, set)
                .unwrap()
                .lines
                .iter()
                .any(|line| line.valid)
        }));
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
        assert!(caches.set(0, CacheRole::Instruction, usize::MAX).is_none());
        if caches.level_count() > 1 {
            assert!(caches.cache(1, CacheRole::Instruction).is_none());
            assert!(caches.cache(1, CacheRole::Data).is_none());
        }
    }
}

/// A machine running `source`, reached only through the public trait — no
/// backend type in sight, which is the point of every test below.
fn trait_machine(id: &str, source: &str) -> Box<dyn raven_engine::Machine> {
    let architecture = ArchitectureRegistry::builtin().get(id).unwrap();
    let engine = Engine::new(architecture.clone());
    let image = engine.assemble(source, 0).unwrap();
    let mut machine = engine
        .create_machine(architecture.descriptor().default_memory_size)
        .unwrap();
    machine.load(&image).unwrap();
    machine
}

/// RV32 through the trait must be the *real* RV32, not a reduced interpreter.
/// A host that sees only `dyn Machine` gets the pipeline model, so it never
/// needs to reach past the trait to draw RV32's microarchitecture.
#[test]
fn riscv32_exposes_its_pipeline_through_the_trait() {
    let machine = trait_machine(riscv32::ID, ".text\n    li a0, 1\n    halt\n");
    let pipeline = machine
        .pipeline()
        .expect("RV32 declares a pipeline capability");
    // A real model behind the accessor, not a stub: named stages a host can
    // draw, each reachable by index.
    assert!(pipeline.stage_count() > 0);
    for index in 0..pipeline.stage_count() {
        let stage = pipeline.stage(index).expect("every stage is addressable");
        assert!(!stage.name.is_empty());
    }
}

/// Every built-in exposes a real, clockable pipeline through the same traits.
#[test]
fn teaching_backends_expose_and_run_their_pipelines() {
    for (id, source, expected_output, expected_stages) in [
        ("sap", "LDI 5\nOUT\nHLT\n", b"5\n".as_slice(), 3),
        (
            "toy16",
            "li r0, 20\nli r1, 22\nadd r2, r0, r1\nprint r2\nhalt\n",
            b"42\n".as_slice(),
            5,
        ),
    ] {
        let mut machine = trait_machine(id, source);
        let pipeline = machine.pipeline().expect("pipeline capability");
        assert_eq!(pipeline.stage_count(), expected_stages);
        machine
            .pipeline_control()
            .expect("pipeline controls")
            .set_enabled(true);

        let mut halted = false;
        for _ in 0..200 {
            if machine.cycle().unwrap().outcome == StepOutcome::Halted {
                halted = true;
                break;
            }
        }
        assert!(halted, "{id} pipeline did not halt");
        assert_eq!(machine.snapshot().stdout, expected_output);
        let pipeline = machine.pipeline().unwrap();
        assert!(pipeline.stats().cycles > pipeline.stats().committed);
        assert!(pipeline.timeline_len() > 0);
    }
}

#[test]
fn teaching_pipelines_flush_taken_branch_paths() {
    for (id, source, expected_output) in [
        (
            "sap",
            "LDI 1\nJZ wrong\nJMP done\nwrong: LDI 9\ndone: OUT\nHLT\n",
            b"1\n".as_slice(),
        ),
        (
            "toy16",
            "li r0, 1\njnz r0, done\nli r1, 99\ndone: li r1, 42\nprint r1\nhalt\n",
            b"42\n".as_slice(),
        ),
    ] {
        let mut machine = trait_machine(id, source);
        machine.pipeline_control().unwrap().set_enabled(true);
        for _ in 0..200 {
            if machine.cycle().unwrap().outcome == StepOutcome::Halted {
                break;
            }
        }
        assert_eq!(machine.snapshot().stdout, expected_output, "{id}");
        assert!(machine.pipeline().unwrap().stats().flushes > 0, "{id}");
    }
}

/// Run `source` to completion under the pipeline and report what the operand
/// hazard logic did: whether any cycle forwarded, and the final statistics.
///
/// Traces last one cycle, so a forward has to be observed while it happens.
fn run_pipelined(id: &str, source: &str) -> (bool, raven_engine::capability::PipelineStats) {
    let mut machine = trait_machine(id, source);
    machine.pipeline_control().unwrap().set_enabled(true);
    let mut forwarded = false;
    for _ in 0..200 {
        let halted = machine.cycle().unwrap().outcome == StepOutcome::Halted;
        let pipeline = machine.pipeline().unwrap();
        forwarded |= (0..pipeline.trace_count()).any(|index| {
            pipeline.trace(index).unwrap().kind
                == raven_engine::capability::PipelineTraceKind::Forward
        });
        if halted {
            break;
        }
    }
    let stats = machine.pipeline().unwrap().stats();
    (forwarded, stats)
}

/// A result the datapath already computed reaches the next instruction over a
/// bypass, so a back-to-back dependency costs nothing. Without forwarding every
/// one of these cost a stall, which is most of what a teaching program does.
#[test]
fn teaching_pipelines_forward_a_result_the_datapath_already_has() {
    let (forwarded, stats) = run_pipelined(
        "toy16",
        "li r0, 20\nli r1, 22\nadd r2, r0, r1\nprint r2\nhalt\n",
    );
    assert!(forwarded, "a dependent ALU pair never forwarded: {stats:?}");
    assert_eq!(stats.load_use_stalls, 0, "{stats:?}");
}

/// The one dependency a bypass cannot cover: a load's data arrives at the end
/// of the memory stage, so the instruction right behind it waits a cycle.
#[test]
fn teaching_pipelines_stall_only_for_a_load_result_that_is_not_ready() {
    let (_, stats) = run_pipelined(
        "toy16",
        "li r0, 7\nstore r0, [200]\nload r1, [200]\nadd r2, r1, r1\nprint r2\nhalt\n",
    );
    assert!(stats.load_use_stalls > 0, "{stats:?}");
}

/// A host draws the datapath from `edges`, so the bypasses have to be declared,
/// not just simulated. SAP's execute stage is its last one, so it has none to
/// declare — and must not invent an edge pointing off the end of the pipeline.
#[test]
fn teaching_pipelines_declare_the_bypasses_they_have() {
    use raven_engine::capability::PipelineEdgeKind;

    for (id, source, expected_bypasses) in
        [("toy16", "li r0, 1\nhalt\n", 2), ("sap", "LDI 1\nHLT\n", 0)]
    {
        let machine = trait_machine(id, source);
        let pipeline = machine.pipeline().unwrap();
        let edges = pipeline.edges();
        let bypasses: Vec<_> = edges
            .iter()
            .filter(|edge| edge.kind == PipelineEdgeKind::Forward)
            .collect();
        assert_eq!(bypasses.len(), expected_bypasses, "{id}: {edges:?}");
        for edge in bypasses {
            assert!(edge.from < pipeline.stage_count(), "{id}: {edge:?}");
            assert!(
                edge.to < edge.from,
                "{id}: a bypass runs backwards: {edge:?}"
            );
        }
    }
}

/// Register and memory edits made through the trait must land in the same
/// runtime execution reads from — the bug a second, parallel RV32 would cause.
#[test]
fn riscv32_trait_edits_are_visible_to_execution() {
    let mut machine = trait_machine(riscv32::ID, ".text\n    add a0, a0, a1\n    halt\n");
    machine.write_register("a0", 40).unwrap();
    machine.write_register("a1", 2).unwrap();
    machine.step().unwrap();

    let registers = machine.registers().unwrap();
    let a0 = registers.resolve("a0").unwrap();
    assert_eq!(registers.read(a0), Some(42));

    machine.write_memory(0x400, &[0xEF, 0xBE]).unwrap();
    assert_eq!(machine.read_memory(0x400, 2).unwrap(), vec![0xEF, 0xBE]);
}

/// A host holding `dyn Machine` can ask for the concrete backend back, and the
/// runtime it gets is the one the trait has been reading all along — not a
/// second copy. That is what lets a host keep RV32-only execution controls
/// (breakpoints, step-back, harts) without owning a parallel runtime.
#[test]
fn riscv32_hands_its_runtime_back_through_the_trait_object() {
    let mut machine = trait_machine(riscv32::ID, ".text\n    li a0, 42\n    halt\n");
    machine.step().unwrap();

    let rv32 = (machine.as_ref() as &dyn std::any::Any)
        .downcast_ref::<riscv32::RiscV32Machine>()
        .expect("an RV32 machine downcasts to RV32");
    assert_eq!(rv32.falcon().cpu().read(10), 42);
    assert_eq!(u64::from(rv32.falcon().cpu().pc), machine.snapshot().pc);

    // Every other backend answers `None` rather than something plausible.
    let toy16 = trait_machine("toy16", "li r0, 1\nhalt");
    assert!(
        (toy16.as_ref() as &dyn std::any::Any)
            .downcast_ref::<riscv32::RiscV32Machine>()
            .is_none()
    );
}

/// `x0` is hardwired; the runtime's own rule must reach trait callers rather
/// than being re-implemented (and allowed to drift) at the trait boundary.
#[test]
fn riscv32_refuses_to_write_x0_through_the_trait() {
    let mut machine = trait_machine(riscv32::ID, ".text\n    halt\n");
    assert!(machine.write_register("zero", 7).is_err());
    let registers = machine.registers().unwrap();
    assert_eq!(registers.read(RegisterId::new(0, 0)), Some(0));
}

/// The listing badges instructions and the inspector lists fields straight off
/// `inspect`, so the encoding formats and the scattered B/J immediates have to
/// be right here or every RV32 row is wrong.
#[test]
fn riscv32_inspect_reports_encoding_formats_and_immediates() {
    let codec = riscv32::RiscV32Codec;
    let field = |bytes: &[u8], name: &str| {
        codec
            .inspect(0, bytes)
            .and_then(|info| {
                info.fields
                    .iter()
                    .find(|f| f.name == name)
                    .map(|f| f.value.clone())
            })
            .unwrap_or_default()
    };
    let class = |word: u32| codec.inspect(0, &word.to_le_bytes()).map(|i| i.class);

    // add x1, x2, x3 — R-type
    assert_eq!(class(0x003100B3), Some("R"));
    // addi x1, x2, -1 — I-type, immediate sign-extends
    let addi = 0xFFF10093u32.to_le_bytes();
    assert_eq!(class(0xFFF10093), Some("I"));
    assert_eq!(field(&addi, "imm"), "-1");
    // beq x0, x0, -4 — B-type immediate is scattered across four fields
    let beq = 0xFE000EE3u32.to_le_bytes();
    assert_eq!(class(0xFE000EE3), Some("B"));
    assert_eq!(field(&beq, "imm"), "-4");
    // jal x0, 8 — J-type, likewise scattered
    let jal = 0x0080006Fu32.to_le_bytes();
    assert_eq!(class(0x0080006F), Some("J"));
    assert_eq!(field(&jal, "imm"), "8");

    // Bytes that decode to nothing must not be given a format.
    assert!(codec.inspect(0, &[0xFF, 0xFF, 0xFF, 0xFF]).is_none());
}

/// The field map draws `layout` left to right over the instruction's bits, so
/// a backend whose segments do not sum to its width would render a lie. Every
/// backend is held to this, not just RV32 — that is what lets one renderer
/// serve all of them.
#[test]
fn every_backend_describes_a_complete_instruction_bit_layout() {
    for (id, source) in [
        (riscv32::ID, ".text\n    li a0, 42\n    halt\n"),
        ("toy16", "li r0, 42\nprint r0\nhalt"),
        ("sap", "LDI 5\nOUT\nHLT\n"),
    ] {
        let architecture = ArchitectureRegistry::builtin().get(id).unwrap();
        let engine = Engine::new(architecture.clone());
        let image = engine.assemble(source, 0).unwrap();
        let mut machine = engine
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        machine.load(&image).unwrap();

        let code = machine.code().unwrap_or_else(|| panic!("{id}: no codec"));
        let memory = machine.memory().unwrap();
        let bytes = memory.peek(0, 8);
        let info = code
            .inspect(0, &bytes)
            .unwrap_or_else(|| panic!("{id}: first instruction did not inspect"));

        assert!(
            info.layout_is_complete(),
            "{id}: layout covers {} bits, encoding is {}",
            info.layout.iter().map(|s| u32::from(s.width)).sum::<u32>(),
            info.encoding_bits,
        );
        assert!(
            info.layout.iter().all(|s| !s.label.is_empty()),
            "{id}: a layout segment has no label to draw"
        );
    }
}

/// The pipeline is a graph the host lays out, so the graph has to be coherent:
/// edges must name real stages, and every stage must appear exactly once in the
/// layout order or it would silently vanish from the diagram.
#[test]
fn the_pipeline_graph_is_well_formed() {
    let architecture = ArchitectureRegistry::builtin().get(riscv32::ID).unwrap();
    let machine = Engine::new(architecture.clone())
        .create_machine(architecture.descriptor().default_memory_size)
        .unwrap();
    let pipeline = machine.pipeline().unwrap();

    let count = pipeline.stage_count();
    for edge in pipeline.edges() {
        assert!(
            edge.from < count && edge.to < count,
            "dangling edge {edge:?}"
        );
    }

    let mut order = pipeline.stage_order();
    assert_eq!(order.len(), count, "layout order must cover every stage");
    order.sort_unstable();
    order.dedup();
    assert_eq!(order.len(), count, "a stage appears twice in layout order");

    // RV32 declares the branch redirect, so the graph carries more than the
    // straight chain the default would give.
    assert!(
        pipeline
            .edges()
            .iter()
            .any(|e| e.kind == PipelineEdgeKind::Feedback),
        "RV32 should describe its branch-redirect path"
    );
}

/// A host must lay out a pipeline that is not a straight line. This stands in
/// for a backend Raven does not ship yet — the point is that nothing in the
/// contract assumes stages sit in a row in index order.
#[test]
fn a_branching_pipeline_lays_out_in_flow_order() {
    struct Superscalar;

    // 0: fetch → 1: decode → {2: ALU, 3: memory} → 4: commit
    impl PipelineInspect for Superscalar {
        fn status(&self) -> PipelineStatus {
            PipelineStatus {
                enabled: true,
                sequential: false,
                halted: false,
                faulted: false,
            }
        }
        fn stats(&self) -> PipelineStats {
            PipelineStats::default()
        }
        fn stage_count(&self) -> usize {
            5
        }
        fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
            let (name, role) = match index {
                0 => ("F", PipelineStageRole::Fetch),
                1 => ("D", PipelineStageRole::Decode),
                2 => ("ALU", PipelineStageRole::Execute),
                3 => ("MEM", PipelineStageRole::Memory),
                4 => ("C", PipelineStageRole::Commit),
                _ => return None,
            };
            Some(PipelineStageView {
                name,
                slot: None,
                role,
            })
        }
        fn edges(&self) -> Vec<PipelineEdge> {
            vec![
                PipelineEdge::sequential(0, 1),
                PipelineEdge::sequential(1, 2),
                PipelineEdge::sequential(1, 3),
                PipelineEdge::sequential(2, 4),
                PipelineEdge::sequential(3, 4),
            ]
        }
        fn unit_count(&self) -> usize {
            0
        }
        fn unit(&self, _: usize) -> Option<PipelineUnitView<'_>> {
            None
        }
        fn trace_count(&self) -> usize {
            0
        }
        fn trace(&self, _: usize) -> Option<PipelineTraceView<'_>> {
            None
        }
        fn status_message(&self) -> Option<&str> {
            None
        }
        fn timeline_len(&self) -> usize {
            0
        }
        fn timeline_row(&self, _: usize) -> Option<PipelineTimelineRow<'_>> {
            None
        }
        fn timeline_cell(&self, _: usize, _: usize) -> Option<PipelineTimelineCell<'_>> {
            None
        }
    }

    let pipeline = Superscalar;
    assert_eq!(pipeline.entry_stages(), vec![0], "only fetch has no feeder");

    // Fetch and decode come first, the parallel pair next, commit last — and
    // every stage appears exactly once even though the graph forks and rejoins.
    let order = pipeline.stage_order();
    assert_eq!(order.len(), 5);
    assert_eq!(&order[..2], &[0, 1]);
    assert_eq!(*order.last().unwrap(), 4);
    assert!(order[2..4].contains(&2) && order[2..4].contains(&3));
}

/// Translation is described, not assumed: a host reads the scheme's widths and
/// levels rather than knowing what Sv32 is.
#[test]
fn riscv32_describes_its_paging_scheme() {
    let architecture = ArchitectureRegistry::builtin().get(riscv32::ID).unwrap();
    let machine = Engine::new(architecture.clone())
        .create_machine(architecture.descriptor().default_memory_size)
        .unwrap();
    let translation = machine
        .translation()
        .expect("RV32 has an MMU even when paging is off");

    // Paging starts off, and the scheme says so rather than the pane vanishing.
    assert!(!translation.enabled());
    assert_eq!(translation.scheme().name, "bare");
    assert!(translation.root_table().is_none());

    // With translation off every address is its own physical address.
    let probe = translation.probe(0x1234);
    assert_eq!(probe.outcome, TranslationOutcome::Identity);
    assert_eq!(probe.physical, Some(0x1234));

    // The TLB geometry is still reportable, so the pane can draw empty ways.
    assert!(translation.tlb_len() > 0);
    assert!(translation.tlb_sets() >= 1);
    assert_eq!(translation.tlb_stats().hits, 0);
}

/// Backends with no MMU must say so rather than report a fake one.
#[test]
fn backends_without_translation_answer_none() {
    for id in ["sap", "toy16"] {
        let architecture = ArchitectureRegistry::builtin().get(id).unwrap();
        let machine = Engine::new(architecture.clone())
            .create_machine(architecture.descriptor().default_memory_size)
            .unwrap();
        assert!(
            machine.translation().is_none(),
            "{id} claims address translation"
        );
        // …and its descriptor agrees, which is what gates the tab.
        assert!(!architecture.descriptor().capabilities.virtual_memory);
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
