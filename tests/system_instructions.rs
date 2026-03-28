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
