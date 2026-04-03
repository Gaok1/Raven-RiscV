use super::common::{blank, h1, h2, mono, note, thead, trow_wrapped, tsep};
use crate::ui::app::DocsLang;
use ratatui::prelude::*;

fn add_row(
    lines: &mut Vec<Line<'static>>,
    a7: &'static str,
    name: &'static str,
    args: &'static str,
    ret: &'static str,
    notes: &'static str,
) {
    lines.extend(trow_wrapped(a7, name, args, ret, notes));
}

pub(crate) fn syscall_lines(lang: DocsLang) -> Vec<Line<'static>> {
    match lang {
        DocsLang::En => syscall_lines_en(),
        DocsLang::PtBr => syscall_lines_ptbr(),
    }
}

fn syscall_lines_en() -> Vec<Line<'static>> {
    let mut lines = vec![
        h1("RAVEN — Syscall Reference"),
        blank(),
        note("Calling convention: a7 = syscall number · a0..a5 = arguments · a0 = return value"),
        note("Negative return values signal errors (Linux errno convention, e.g. -9 = EBADF)."),
        blank(),
        h2("Linux-compatible syscalls"),
        blank(),
        thead(),
        tsep(),
    ];
    add_row(
        &mut lines,
        "63",
        "read",
        "fd=a0, buf=a1, n=a2",
        "bytes read",
        "fd=0 (stdin) only; blocks until line ready",
    );
    add_row(
        &mut lines,
        "64",
        "write",
        "fd=a0, buf=a1, n=a2",
        "bytes written",
        "fd=1 (stdout) or 2 (stderr)",
    );
    add_row(
        &mut lines,
        "93",
        "exit",
        "code=a0",
        "—",
        "stops the whole program; sets exit code",
    );
    add_row(
        &mut lines,
        "94",
        "exit_group",
        "code=a0",
        "—",
        "same as exit (93)",
    );
    add_row(
        &mut lines,
        "278",
        "getrandom",
        "buf=a0, len=a1, flags=a2",
        "len",
        "fills buf with cryptographic random bytes",
    );
    add_row(
        &mut lines,
        "66",
        "writev",
        "fd=a0, iov=a1, n=a2",
        "bytes written",
        "scatter-write; iovec={u32 base, u32 len}; fd=1/2 only",
    );
    add_row(
        &mut lines,
        "172",
        "getpid",
        "—",
        "1",
        "always returns pid 1",
    );
    add_row(
        &mut lines,
        "174",
        "getuid",
        "—",
        "0",
        "always returns uid 0",
    );
    add_row(
        &mut lines,
        "176",
        "getgid",
        "—",
        "0",
        "always returns gid 0",
    );
    add_row(
        &mut lines,
        "215",
        "munmap",
        "addr=a0, len=a1",
        "0",
        "no-op; memory is never freed in Raven",
    );
    add_row(
        &mut lines,
        "222",
        "mmap",
        "0,len,prot,flags,fd=-1,0",
        "ptr",
        "anon heap alloc (MAP_ANONYMOUS=0x20 required)",
    );
    add_row(
        &mut lines,
        "403",
        "clock_gettime",
        "clockid=a0, *tp=a1",
        "0",
        "writes {tv_sec,tv_nsec} based on instr_count; ~10ns/instr",
    );
    lines.extend([
        blank(),
        note("Supported getrandom flags: GRND_NONBLOCK (0x1), GRND_RANDOM (0x2)."),
        blank(),
        h2("RAVEN teaching extensions  (a7 ≥ 1000)"),
        blank(),
        thead(),
        tsep(),
    ]);
    add_row(
        &mut lines,
        "1000",
        "print_int",
        "a0=integer",
        "—",
        "prints a0 as signed decimal to console",
    );
    add_row(
        &mut lines,
        "1001",
        "print_zstr",
        "a0=addr",
        "—",
        "prints NUL-terminated string at addr",
    );
    add_row(
        &mut lines,
        "1002",
        "print_zstr_ln",
        "a0=addr",
        "—",
        "same as 1001 + appends newline",
    );
    add_row(
        &mut lines,
        "1003",
        "read_line_z",
        "a0=addr",
        "—",
        "reads console line into addr (NUL-terminated); blocks",
    );
    add_row(
        &mut lines,
        "1004",
        "print_uint",
        "a0=u32",
        "—",
        "prints a0 as unsigned decimal",
    );
    add_row(
        &mut lines,
        "1005",
        "print_hex",
        "a0=u32",
        "—",
        "prints a0 as hex  e.g. 0xDEADBEEF",
    );
    add_row(
        &mut lines,
        "1006",
        "print_char",
        "a0=ascii",
        "—",
        "prints one ASCII character",
    );
    add_row(
        &mut lines,
        "1008",
        "print_newline",
        "—",
        "—",
        "prints a newline",
    );
    add_row(
        &mut lines,
        "1010",
        "read_u8",
        "a0=addr",
        "—",
        "reads decimal from console; stores 1 byte at addr",
    );
    add_row(
        &mut lines,
        "1011",
        "read_u16",
        "a0=addr",
        "—",
        "reads decimal from console; stores 2 bytes at addr",
    );
    add_row(
        &mut lines,
        "1012",
        "read_u32",
        "a0=addr",
        "—",
        "reads decimal from console; stores 4 bytes at addr",
    );
    add_row(
        &mut lines,
        "1013",
        "read_int",
        "a0=addr",
        "—",
        "reads signed int (accepts negatives); stores 4 bytes",
    );
    add_row(
        &mut lines,
        "1014",
        "read_float",
        "a0=addr",
        "—",
        "reads f32 from console; stores 4 bytes (IEEE 754)",
    );
    add_row(
        &mut lines,
        "1015",
        "print_float",
        "fa0=f32",
        "—",
        "prints fa0 as float (up to 6 significant digits)",
    );
    add_row(
        &mut lines,
        "1030",
        "get_instr_count",
        "—",
        "a0=count",
        "returns instructions executed since start (low 32 bits)",
    );
    add_row(
        &mut lines,
        "1031",
        "get_cycle_count",
        "—",
        "a0=count",
        "returns total cycles elapsed; pipeline mode uses the pipeline wall-clock",
    );
    add_row(
        &mut lines,
        "1100",
        "hart_start",
        "a0=entry_pc, a1=stack_ptr, a2=arg",
        "a0=hart_id / <0 err",
        "starts a new hart (hardware thread) on a free core; 1:1 hart/core",
    );
    add_row(
        &mut lines,
        "1101",
        "hart_exit",
        "—",
        "—",
        "stops only the current hart; other harts keep running",
    );
    lines.extend([
        blank(),
        h2("RAVEN memory utilities  (a7 ≥ 1050)"),
        blank(),
        thead(),
        tsep(),
    ]);
    add_row(
        &mut lines,
        "1050",
        "memset",
        "a0=dst, a1=byte, a2=len",
        "—",
        "fills len bytes at dst with byte value",
    );
    add_row(
        &mut lines,
        "1051",
        "memcpy",
        "a0=dst, a1=src, a2=len",
        "—",
        "copies len bytes from src to dst",
    );
    add_row(
        &mut lines,
        "1052",
        "strlen",
        "a0=addr",
        "a0=len",
        "returns length of NUL-terminated string",
    );
    add_row(
        &mut lines,
        "1053",
        "strcmp",
        "a0=s1, a1=s2",
        "a0=cmp",
        "compares strings; <0 / 0 / >0",
    );
    lines.extend([
        blank(),
        h2("Example — write(1, buf, 5) via raw ecall"),
        blank(),
        mono("  .data"),
        mono("  msg: .ascii \"hello\""),
        mono("  .text"),
        mono("      la   a1, msg      ; a1 = address of msg"),
        mono("      li   a0, 1        ; a0 = fd 1 (stdout)"),
        mono("      li   a2, 5        ; a2 = 5 bytes"),
        mono("      li   a7, 64       ; a7 = write"),
        mono("      ecall             ; a0 = bytes written (5)"),
        blank(),
        note(
            "Pseudo-instructions like print, print_str, read, etc. expand to these ecalls automatically.",
        ),
    ]);
    lines
}

fn syscall_lines_ptbr() -> Vec<Line<'static>> {
    let mut lines = vec![
        h1("RAVEN — Referência de Syscalls"),
        blank(),
        note("Convenção: a7 = número da syscall · a0..a5 = argumentos · a0 = valor de retorno"),
        note("Retornos negativos indicam erros (convenção Linux errno, ex.: -9 = EBADF)."),
        blank(),
        h2("Syscalls compatíveis com Linux"),
        blank(),
        thead(),
        tsep(),
    ];
    add_row(
        &mut lines,
        "63",
        "read",
        "fd=a0, buf=a1, n=a2",
        "bytes lidos",
        "fd=0 (stdin); bloqueia até linha disponível",
    );
    add_row(
        &mut lines,
        "64",
        "write",
        "fd=a0, buf=a1, n=a2",
        "bytes escritos",
        "fd=1 (stdout) ou 2 (stderr)",
    );
    add_row(
        &mut lines,
        "93",
        "exit",
        "code=a0",
        "—",
        "encerra o programa inteiro; define código de saída",
    );
    add_row(
        &mut lines,
        "94",
        "exit_group",
        "code=a0",
        "—",
        "igual a exit (93)",
    );
    add_row(
        &mut lines,
        "278",
        "getrandom",
        "buf=a0, len=a1, flags=a2",
        "len",
        "preenche buf com bytes aleatórios criptográficos",
    );
    add_row(
        &mut lines,
        "66",
        "writev",
        "fd=a0, iov=a1, n=a2",
        "bytes escritos",
        "scatter-write; iovec={u32 base, u32 len}; fd=1/2",
    );
    add_row(&mut lines, "172", "getpid", "—", "1", "retorna pid fixo 1");
    add_row(&mut lines, "174", "getuid", "—", "0", "retorna uid fixo 0");
    add_row(&mut lines, "176", "getgid", "—", "0", "retorna gid fixo 0");
    add_row(
        &mut lines,
        "215",
        "munmap",
        "addr=a0, len=a1",
        "0",
        "nop; memória não é liberada no Raven",
    );
    add_row(
        &mut lines,
        "222",
        "mmap",
        "0,len,prot,flags,fd=-1,0",
        "ptr",
        "aloca do heap anonimamente (MAP_ANONYMOUS=0x20)",
    );
    add_row(
        &mut lines,
        "403",
        "clock_gettime",
        "clockid=a0, *tp=a1",
        "0",
        "escreve {tv_sec,tv_nsec} com base em instr_count",
    );
    lines.extend([
        blank(),
        note("Flags aceitas em getrandom: GRND_NONBLOCK (0x1), GRND_RANDOM (0x2)."),
        blank(),
        h2("Extensões didáticas do RAVEN  (a7 ≥ 1000)"),
        blank(),
        thead(),
        tsep(),
    ]);
    add_row(
        &mut lines,
        "1000",
        "print_int",
        "a0=inteiro",
        "—",
        "imprime a0 como decimal com sinal no console",
    );
    add_row(
        &mut lines,
        "1001",
        "print_zstr",
        "a0=endereço",
        "—",
        "imprime string terminada em NUL no endereço",
    );
    add_row(
        &mut lines,
        "1002",
        "print_zstr_ln",
        "a0=endereço",
        "—",
        "igual a 1001 + adiciona nova linha",
    );
    add_row(
        &mut lines,
        "1003",
        "read_line_z",
        "a0=endereço",
        "—",
        "lê linha do console em addr (NUL no fim); bloqueia",
    );
    add_row(
        &mut lines,
        "1004",
        "print_uint",
        "a0=u32",
        "—",
        "imprime a0 como decimal sem sinal",
    );
    add_row(
        &mut lines,
        "1005",
        "print_hex",
        "a0=u32",
        "—",
        "imprime a0 em hex, ex.: 0xDEADBEEF",
    );
    add_row(
        &mut lines,
        "1006",
        "print_char",
        "a0=ascii",
        "—",
        "imprime um caractere ASCII",
    );
    add_row(
        &mut lines,
        "1008",
        "print_newline",
        "—",
        "—",
        "imprime nova linha",
    );
    add_row(
        &mut lines,
        "1010",
        "read_u8",
        "a0=endereço",
        "—",
        "lê decimal do console; armazena 1 byte em addr",
    );
    add_row(
        &mut lines,
        "1011",
        "read_u16",
        "a0=endereço",
        "—",
        "lê decimal do console; armazena 2 bytes em addr",
    );
    add_row(
        &mut lines,
        "1012",
        "read_u32",
        "a0=endereço",
        "—",
        "lê decimal do console; armazena 4 bytes em addr",
    );
    add_row(
        &mut lines,
        "1013",
        "read_int",
        "a0=endereço",
        "—",
        "lê inteiro com sinal (aceita negativos); armazena 4 bytes",
    );
    add_row(
        &mut lines,
        "1014",
        "read_float",
        "a0=endereço",
        "—",
        "lê f32 do console; armazena 4 bytes (IEEE 754)",
    );
    add_row(
        &mut lines,
        "1015",
        "print_float",
        "fa0=f32",
        "—",
        "imprime fa0 como float (até 6 dígitos significativos)",
    );
    add_row(
        &mut lines,
        "1030",
        "get_instr_count",
        "—",
        "a0=count",
        "retorna instruções executadas desde o início (32 bits)",
    );
    add_row(
        &mut lines,
        "1031",
        "get_cycle_count",
        "—",
        "a0=count",
        "retorna ciclos totais decorridos; em pipeline usa o clock global do pipeline",
    );
    add_row(
        &mut lines,
        "1100",
        "hart_start",
        "a0=entry_pc, a1=stack_ptr, a2=arg",
        "a0=hart_id / <0 erro",
        "inicia novo hart (hardware thread) em um core livre; modelo 1:1",
    );
    add_row(
        &mut lines,
        "1101",
        "hart_exit",
        "—",
        "—",
        "encerra apenas o hart atual; os demais continuam rodando",
    );
    lines.extend([
        blank(),
        h2("Utilitários de memória do RAVEN  (a7 ≥ 1050)"),
        blank(),
        thead(),
        tsep(),
    ]);
    add_row(
        &mut lines,
        "1050",
        "memset",
        "a0=dst, a1=byte, a2=len",
        "—",
        "preenche len bytes em dst com o valor byte",
    );
    add_row(
        &mut lines,
        "1051",
        "memcpy",
        "a0=dst, a1=src, a2=len",
        "—",
        "copia len bytes de src para dst",
    );
    add_row(
        &mut lines,
        "1052",
        "strlen",
        "a0=endereço",
        "a0=len",
        "retorna comprimento de string terminada em NUL",
    );
    add_row(
        &mut lines,
        "1053",
        "strcmp",
        "a0=s1, a1=s2",
        "a0=cmp",
        "compara strings; <0 / 0 / >0",
    );
    lines.extend([
        blank(),
        h2("Exemplo — write(1, buf, 5) via ecall direto"),
        blank(),
        mono("  .data"),
        mono("  msg: .ascii \"hello\""),
        mono("  .text"),
        mono("      la   a1, msg      ; a1 = endereço de msg"),
        mono("      li   a0, 1        ; a0 = fd 1 (stdout)"),
        mono("      li   a2, 5        ; a2 = 5 bytes"),
        mono("      li   a7, 64       ; a7 = write"),
        mono("      ecall             ; a0 = bytes escritos (5)"),
        blank(),
        note(
            "Pseudo-instruções como print, print_str, read, etc. expandem para esses ecalls automaticamente.",
        ),
    ]);
    lines
}
