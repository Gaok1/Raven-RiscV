use super::*;

#[test]
fn store_and_load_word() {
    let mut ram = Ram::new(64);
    ram.store32(0x10, 0xDEADBEEF).unwrap();
    assert_eq!(ram.load32(0x10).unwrap(), 0xDEADBEEF);
}
