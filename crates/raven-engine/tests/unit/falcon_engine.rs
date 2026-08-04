use super::*;

#[test]
fn runs_asm_and_exposes_state() {
    // store 42 to mem[0x100], print it, exit(42).
    let r = Falcon::new()
        .asm(
            "\
            .text\n\
            li   t0, 42\n\
            li   t1, 0x100\n\
            sw   t0, 0(t1)\n\
            li   a7, 1000\n\
            mv   a0, t0\n\
            ecall\n\
            li   a0, 42\n\
            li   a7, 93\n\
            ecall\n",
        )
        .run()
        .unwrap();

    assert_eq!(r.exit_code, Some(42));
    assert!(!r.timed_out);
    assert_eq!(r.reg("a0"), 42);
    assert_eq!(r.reg("t0"), 42);
    assert_eq!(r.read_word(0x100), 42);
    assert_eq!(r.stdout(), "42");
}

#[test]
fn multihart_is_rejected() {
    let err = Falcon::new().asm(".text\n ecall\n").harts(2).run();
    assert!(err.is_err());
}
