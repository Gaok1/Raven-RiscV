use raven_riscv_engine::{
    ArchitectureRegistry, Engine, MachineState, ProgramImage, ProgramSegment, StepOutcome,
};

#[test]
fn builtin_registry_is_stable_and_runtime_selectable() {
    let registry = ArchitectureRegistry::with_builtins();
    let ids: Vec<_> = registry
        .architectures()
        .iter()
        .map(|a| a.descriptor().id)
        .collect();
    assert_eq!(ids, ["riscv32", "toy16"]);
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

        let mut machine = engine.create_machine(64 * 1024).unwrap();
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
    for id in ["riscv32", "toy16"] {
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
