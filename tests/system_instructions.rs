use raven::falcon::{decoder::decode, encoder::encode, instruction::Instruction};

#[test]
fn halt_and_ebreak_keep_distinct_system_encodings() {
    assert_eq!(encode(Instruction::Halt).unwrap(), 0x0020_0073);
    assert_eq!(encode(Instruction::Ebreak).unwrap(), 0x0010_0073);
    assert_ne!(
        encode(Instruction::Halt).unwrap(),
        encode(Instruction::Ebreak).unwrap()
    );
}

#[test]
fn system_decode_is_canonical_for_halt_and_ebreak() {
    assert!(matches!(decode(0x0020_0073).unwrap(), Instruction::Halt));
    assert!(matches!(decode(0x0010_0073).unwrap(), Instruction::Ebreak));
}

#[test]
fn fence_and_fence_i_keep_distinct_misc_mem_encodings() {
    assert_eq!(encode(Instruction::Fence).unwrap(), 0x0FF0_000F);
    assert_eq!(encode(Instruction::FenceI).unwrap(), 0x0000_100F);
    assert!(matches!(decode(0x0FF0_000F).unwrap(), Instruction::Fence));
    assert!(matches!(decode(0x0000_100F).unwrap(), Instruction::FenceI));
}
