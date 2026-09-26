// falcon/fpu.rs — RV32F arithmetic with IEEE 754 exception flags.
//
// Every operation works on raw f32 bit patterns. It computes the exact result
// in integer arithmetic and rounds it once, in `round_pack`. That single
// rounding step raises the five flags the F extension accumulates in
// `fflags`, and it honors every rounding mode. The rules are the RISC-V ones:
// a NaN result is always the canonical NaN, tininess is detected after
// rounding, and fmin/fmax return the non-NaN operand.
//
// The sequential executor and the pipeline's EX stage both call `execute`,
// so the two modes cannot disagree on a result or a flag.
//
// Arithmetic instructions carry no `rm` field in `Instruction`, so they round
// to nearest-even. Only the float-to-integer conversions honor `rm` and `frm`.

use crate::falcon::instruction::Instruction;

/// Exception flags, in `fflags` bit order.
pub const NX: u8 = 1 << 0; // inexact
pub const UF: u8 = 1 << 1; // underflow
pub const OF: u8 = 1 << 2; // overflow
pub const DZ: u8 = 1 << 3; // divide by zero
pub const NV: u8 = 1 << 4; // invalid operation

/// CSR numbers of the floating-point control and status registers.
pub const CSR_FFLAGS: u16 = 0x001;
pub const CSR_FRM: u16 = 0x002;
pub const CSR_FCSR: u16 = 0x003;

/// The only NaN an F instruction produces.
pub const CANONICAL_NAN: u32 = 0x7FC0_0000;

const SIGN: u32 = 0x8000_0000;
const INF: u32 = 0x7F80_0000;
const MAX_FINITE: u32 = 0x7F7F_FFFF;

/// IEEE rounding modes, in `rm`/`frm` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rm {
    Rne,
    Rtz,
    Rdn,
    Rup,
    Rmm,
}

impl Rm {
    /// Resolve an instruction's `rm` field; 7 (dynamic) reads `frm`.
    /// Reserved encodings fall back to RNE instead of raising an
    /// illegal-instruction exception.
    pub fn resolve(rm: u8, frm: u8) -> Rm {
        match if rm == 7 { frm } else { rm } {
            1 => Rm::Rtz,
            2 => Rm::Rdn,
            3 => Rm::Rup,
            4 => Rm::Rmm,
            _ => Rm::Rne,
        }
    }
}

/// Result bits plus the exception flags the operation raised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpResult {
    pub bits: u32,
    pub flags: u8,
}

fn ok(bits: u32) -> FpResult {
    FpResult { bits, flags: 0 }
}

fn nan(flags: u8) -> FpResult {
    FpResult {
        bits: CANONICAL_NAN,
        flags,
    }
}

fn is_nan(x: u32) -> bool {
    x & 0x7FFF_FFFF > INF
}

fn is_snan(x: u32) -> bool {
    is_nan(x) && x & 0x0040_0000 == 0
}

fn is_inf(x: u32) -> bool {
    x & 0x7FFF_FFFF == INF
}

fn is_zero(x: u32) -> bool {
    x & 0x7FFF_FFFF == 0
}

fn sign(x: u32) -> bool {
    x & SIGN != 0
}

fn signed(bits: u32, neg: bool) -> u32 {
    bits | (neg as u32) << 31
}

/// NV when any operand is a signaling NaN.
fn snan_flag(ops: &[u32]) -> u8 {
    if ops.iter().any(|&x| is_snan(x)) {
        NV
    } else {
        0
    }
}

/// A finite nonzero value as `(exp, sig)` with value `sig * 2^exp` and
/// `sig` in [2^23, 2^24).
fn unpack(x: u32) -> (i32, u64) {
    let e = ((x >> 23) & 0xFF) as i32;
    let f = (x & 0x007F_FFFF) as u64;
    if e == 0 {
        let shift = f.leading_zeros() as i32 - 40; // top bit to bit 23
        (-149 - shift, f << shift)
    } else {
        (e - 150, f | 0x0080_0000)
    }
}

/// Drop the low `d` bits of `sig` (d >= 1), plus the sticky bits below it,
/// and round what is left to an integer in mode `rm`.
/// Returns the rounded integer and whether anything was lost.
fn round_shift(neg: bool, sig: u64, sticky: bool, d: i32, rm: Rm) -> (u64, bool) {
    let (kept, rem, half): (u64, u128, u128) = if d > 64 {
        // `sig` is nonzero and lies entirely below half a unit.
        (0, 1, 1 << 64)
    } else {
        let kept = if d == 64 { 0 } else { sig >> d };
        let rem = sig as u128 & ((1u128 << d) - 1);
        (kept, rem, 1u128 << (d - 1))
    };
    let inexact = rem != 0 || sticky;
    let above_half = rem > half || (rem == half && sticky);
    let at_half = rem == half && !sticky;
    let up = match rm {
        Rm::Rne => above_half || (at_half && kept & 1 == 1),
        Rm::Rtz => false,
        Rm::Rdn => inexact && neg,
        Rm::Rup => inexact && !neg,
        Rm::Rmm => above_half || at_half,
    };
    (kept + up as u64, inexact)
}

/// Round `sig * 2^exp` (sig != 0) to f32. `sticky` stands for nonzero bits
/// below `sig`'s last bit. Callers that set it keep at least 26 significant
/// bits in `sig`, so the sticky sits below the rounding point.
fn round_pack(neg: bool, exp: i32, sig: u64, sticky: bool, rm: Rm) -> FpResult {
    debug_assert!(sig != 0);
    debug_assert!(!sticky || sig >> 25 != 0);
    let lz = sig.leading_zeros() as i32;
    let (exp, sig) = (exp - lz, sig << lz);
    let top = exp + 63; // the value lies in [2^top, 2^(top+1))

    // Weight of the result's last bit: 24 significant bits for a normal
    // result, fixed at 2^-149 for a subnormal one.
    let mut lsb = (top - 23).max(-149);
    let (mut kept, inexact) = round_shift(neg, sig, sticky, lsb - exp, rm);
    if kept == 1 << 24 {
        // Rounding carried into a new bit.
        kept >>= 1;
        lsb += 1;
    }

    // Tininess after rounding: rounded to 24 bits with an unbounded exponent,
    // the value would still be below 2^-126.
    let tiny = top < -126
        && !(top == -127 && round_shift(neg, sig, sticky, (top - 23) - exp, rm).0 == 1 << 24);

    let biased = if kept >= 1 << 23 { lsb + 150 } else { 0 };
    if biased >= 255 {
        let to_inf = match rm {
            Rm::Rne | Rm::Rmm => true,
            Rm::Rtz => false,
            Rm::Rdn => neg,
            Rm::Rup => !neg,
        };
        let bits = if to_inf { INF } else { MAX_FINITE };
        return FpResult {
            bits: signed(bits, neg),
            flags: OF | NX,
        };
    }
    let mut flags = 0;
    if inexact {
        flags |= NX;
        if tiny {
            flags |= UF;
        }
    }
    let bits = signed((biased as u32) << 23 | (kept as u32 & 0x007F_FFFF), neg);
    FpResult { bits, flags }
}

/// Round `mag * 2^exp` held in 128 bits, folding the low bits into a sticky.
fn round_wide(neg: bool, exp: i32, mag: u128, rm: Rm) -> FpResult {
    let width = 128 - mag.leading_zeros() as i32;
    if width <= 64 {
        return round_pack(neg, exp, mag as u64, false, rm);
    }
    let drop = width - 64;
    let sticky = mag & ((1u128 << drop) - 1) != 0;
    round_pack(neg, exp + drop, (mag >> drop) as u64, sticky, rm)
}

/// Round `±mx*2^ex ± my*2^ey` (both terms nonzero, each at most 48 bits).
fn sum(nx: bool, ex: i32, mx: u64, ny: bool, ey: i32, my: u64, rm: Rm) -> FpResult {
    let top = |e: i32, m: u64| e + 63 - m.leading_zeros() as i32;
    let (tx, ty) = (top(ex, mx), top(ey, my));
    if (tx - ty).abs() > 64 {
        // The smaller term is below a quarter of the larger one's last bit
        // once that one is shifted to bit 62: it only nudges the value.
        let (nb, eb, mb, ns) = if tx > ty {
            (nx, ex, mx, ny)
        } else {
            (ny, ey, my, nx)
        };
        let shift = mb.leading_zeros() as i32 - 1;
        let sig = mb << shift;
        let sig = if nb == ns { sig } else { sig - 1 };
        return round_pack(nb, eb - shift, sig, true, rm);
    }
    let e = ex.min(ey);
    let x = (mx as u128) << (ex - e);
    let y = (my as u128) << (ey - e);
    let (neg, mag) = if nx == ny {
        (nx, x + y)
    } else if x >= y {
        (nx, x - y)
    } else {
        (ny, y - x)
    };
    if mag == 0 {
        // An exact zero sum is +0, except when rounding down.
        return ok(signed(0, rm == Rm::Rdn));
    }
    round_wide(neg, e, mag, rm)
}

pub fn add(a: u32, b: u32, rm: Rm) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return nan(snan_flag(&[a, b]));
    }
    match (is_inf(a), is_inf(b)) {
        (true, true) if sign(a) != sign(b) => return nan(NV),
        (true, _) => return ok(a),
        (_, true) => return ok(b),
        _ => {}
    }
    if is_zero(a) && is_zero(b) {
        let neg = if sign(a) == sign(b) {
            sign(a)
        } else {
            rm == Rm::Rdn
        };
        return ok(signed(0, neg));
    }
    if is_zero(a) {
        return ok(b);
    }
    if is_zero(b) {
        return ok(a);
    }
    let (ea, ma) = unpack(a);
    let (eb, mb) = unpack(b);
    sum(sign(a), ea, ma, sign(b), eb, mb, rm)
}

pub fn sub(a: u32, b: u32, rm: Rm) -> FpResult {
    add(a, b ^ SIGN, rm)
}

pub fn mul(a: u32, b: u32, rm: Rm) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return nan(snan_flag(&[a, b]));
    }
    let neg = sign(a) != sign(b);
    if is_inf(a) || is_inf(b) {
        if is_zero(a) || is_zero(b) {
            return nan(NV);
        }
        return ok(signed(INF, neg));
    }
    if is_zero(a) || is_zero(b) {
        return ok(signed(0, neg));
    }
    let (ea, ma) = unpack(a);
    let (eb, mb) = unpack(b);
    round_pack(neg, ea + eb, ma * mb, false, rm)
}

pub fn div(a: u32, b: u32, rm: Rm) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return nan(snan_flag(&[a, b]));
    }
    let neg = sign(a) != sign(b);
    match (is_inf(a), is_inf(b)) {
        (true, true) => return nan(NV),
        (true, false) => return ok(signed(INF, neg)),
        (false, true) => return ok(signed(0, neg)),
        _ => {}
    }
    if is_zero(b) {
        if is_zero(a) {
            return nan(NV);
        }
        return FpResult {
            bits: signed(INF, neg),
            flags: DZ,
        };
    }
    if is_zero(a) {
        return ok(signed(0, neg));
    }
    let (ea, ma) = unpack(a);
    let (eb, mb) = unpack(b);
    // At least 40 quotient bits; the remainder becomes the sticky.
    let n = ma << 40;
    round_pack(neg, ea - eb - 40, n / mb, n % mb != 0, rm)
}

pub fn sqrt(a: u32, rm: Rm) -> FpResult {
    if is_nan(a) {
        return nan(snan_flag(&[a]));
    }
    if is_zero(a) {
        return ok(a); // sqrt(-0) = -0
    }
    if sign(a) {
        return nan(NV);
    }
    if is_inf(a) {
        return ok(a);
    }
    let (mut e, mut m) = unpack(a);
    if e & 1 != 0 {
        // An even exponent halves exactly.
        m <<= 1;
        e -= 1;
    }
    let wide = (m as u128) << 80;
    let root = isqrt(wide);
    round_pack(false, e / 2 - 40, root as u64, root * root != wide, rm)
}

/// Integer square root, rounded down.
fn isqrt(n: u128) -> u128 {
    let mut r = (n as f64).sqrt() as u128;
    while r * r > n {
        r -= 1;
    }
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// Fused multiply-add: `±(a*b) ± c` with a single rounding.
pub fn fma(a: u32, b: u32, c: u32, neg_prod: bool, neg_c: bool, rm: Rm) -> FpResult {
    // inf * 0 is invalid even when c is a quiet NaN.
    if (is_inf(a) && is_zero(b)) || (is_zero(a) && is_inf(b)) {
        return nan(NV);
    }
    if is_nan(a) || is_nan(b) || is_nan(c) {
        return nan(snan_flag(&[a, b, c]));
    }
    let np = sign(a) ^ sign(b) ^ neg_prod;
    let c = if neg_c { c ^ SIGN } else { c };
    let nc = sign(c);
    if is_inf(a) || is_inf(b) {
        if is_inf(c) && nc != np {
            return nan(NV);
        }
        return ok(signed(INF, np));
    }
    if is_inf(c) {
        return ok(c);
    }
    if is_zero(a) || is_zero(b) {
        if is_zero(c) {
            let neg = if np == nc { np } else { rm == Rm::Rdn };
            return ok(signed(0, neg));
        }
        return ok(c);
    }
    let (ea, ma) = unpack(a);
    let (eb, mb) = unpack(b);
    if is_zero(c) {
        return round_pack(np, ea + eb, ma * mb, false, rm);
    }
    let (ec, mc) = unpack(c);
    sum(np, ea + eb, ma * mb, nc, ec, mc, rm)
}

/// fmin.s / fmax.s: the non-NaN operand wins; -0 is below +0.
fn min_max(a: u32, b: u32, want_min: bool) -> FpResult {
    let bits = match (is_nan(a), is_nan(b)) {
        (true, true) => CANONICAL_NAN,
        (true, false) => b,
        (false, true) => a,
        _ => {
            let (x, y) = (f32::from_bits(a), f32::from_bits(b));
            let pick_a = if x == y {
                sign(a) == want_min
            } else {
                (x < y) == want_min
            };
            if pick_a { a } else { b }
        }
    };
    FpResult {
        bits,
        flags: snan_flag(&[a, b]),
    }
}

pub fn min(a: u32, b: u32) -> FpResult {
    min_max(a, b, true)
}

pub fn max(a: u32, b: u32) -> FpResult {
    min_max(a, b, false)
}

/// feq.s is a quiet comparison: only a signaling NaN raises NV.
pub fn feq(a: u32, b: u32) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return FpResult {
            bits: 0,
            flags: snan_flag(&[a, b]),
        };
    }
    ok((f32::from_bits(a) == f32::from_bits(b)) as u32)
}

/// flt.s and fle.s are signaling: any NaN raises NV.
pub fn flt(a: u32, b: u32) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return FpResult { bits: 0, flags: NV };
    }
    ok((f32::from_bits(a) < f32::from_bits(b)) as u32)
}

pub fn fle(a: u32, b: u32) -> FpResult {
    if is_nan(a) || is_nan(b) {
        return FpResult { bits: 0, flags: NV };
    }
    ok((f32::from_bits(a) <= f32::from_bits(b)) as u32)
}

/// fcvt.w.s / fcvt.wu.s. Out-of-range inputs and NaN saturate and raise NV.
pub fn to_int(a: u32, is_signed: bool, rm: Rm) -> FpResult {
    let (max, min): (i64, i64) = if is_signed {
        (i32::MAX as i64, i32::MIN as i64)
    } else {
        (u32::MAX as i64, 0)
    };
    let saturate = |neg: bool| FpResult {
        bits: (if neg { min } else { max }) as u32,
        flags: NV,
    };
    if is_nan(a) {
        return saturate(false);
    }
    let neg = sign(a);
    if is_inf(a) {
        return saturate(neg);
    }
    if is_zero(a) {
        return ok(0);
    }
    let (e, m) = unpack(a);
    let (mag, inexact) = if e >= 0 {
        if e > 8 {
            return saturate(neg); // at least 2^32
        }
        (m << e, false)
    } else {
        round_shift(neg, m, false, -e, rm)
    };
    let value = if neg { -(mag as i64) } else { mag as i64 };
    if value > max || value < min {
        return saturate(neg);
    }
    FpResult {
        bits: value as u32,
        flags: if inexact { NX } else { 0 },
    }
}

/// fcvt.s.w / fcvt.s.wu.
pub fn from_int(x: u32, is_signed: bool, rm: Rm) -> FpResult {
    let (neg, mag) = if is_signed && (x as i32) < 0 {
        (true, (x as i32).unsigned_abs() as u64)
    } else {
        (false, x as u64)
    };
    if mag == 0 {
        return ok(0);
    }
    round_pack(neg, 0, mag, false, rm)
}

/// fclass.s: one-hot class mask. The quiet bit, not the sign, tells a
/// signaling NaN from a quiet one.
pub fn classify(a: u32) -> u32 {
    let neg = sign(a);
    let frac = a & 0x007F_FFFF;
    let bit = match ((a >> 23) & 0xFF, frac) {
        (0xFF, 0) => {
            if neg {
                0
            } else {
                7
            }
        }
        (0xFF, f) => {
            if f & 0x0040_0000 != 0 {
                9
            } else {
                8
            }
        }
        (0, 0) => {
            if neg {
                3
            } else {
                4
            }
        }
        (0, _) => {
            if neg {
                2
            } else {
                5
            }
        }
        _ => {
            if neg {
                1
            } else {
                6
            }
        }
    };
    1 << bit
}

/// Registers of an F compute instruction as `(rd, rs1, rs2, rs3)`; absent
/// operands are 0. `None` for anything that is not an OP-FP/R4 instruction.
pub fn registers(instr: Instruction) -> Option<(u8, u8, u8, u8)> {
    use Instruction::*;
    Some(match instr {
        FaddS { rd, rs1, rs2 }
        | FsubS { rd, rs1, rs2 }
        | FmulS { rd, rs1, rs2 }
        | FdivS { rd, rs1, rs2 }
        | FminS { rd, rs1, rs2 }
        | FmaxS { rd, rs1, rs2 }
        | FsgnjS { rd, rs1, rs2 }
        | FsgnjnS { rd, rs1, rs2 }
        | FsgnjxS { rd, rs1, rs2 }
        | FeqS { rd, rs1, rs2 }
        | FltS { rd, rs1, rs2 }
        | FleS { rd, rs1, rs2 } => (rd, rs1, rs2, 0),
        FsqrtS { rd, rs1 }
        | FcvtSW { rd, rs1 }
        | FcvtSWu { rd, rs1 }
        | FmvXW { rd, rs1 }
        | FmvWX { rd, rs1 }
        | FclassS { rd, rs1 }
        | FcvtWS { rd, rs1, .. }
        | FcvtWuS { rd, rs1, .. } => (rd, rs1, 0, 0),
        FmaddS { rd, rs1, rs2, rs3 }
        | FmsubS { rd, rs1, rs2, rs3 }
        | FnmsubS { rd, rs1, rs2, rs3 }
        | FnmaddS { rd, rs1, rs2, rs3 } => (rd, rs1, rs2, rs3),
        _ => return None,
    })
}

/// rs1 comes from the integer file (fcvt.s.w, fcvt.s.wu, fmv.w.x).
pub fn reads_int(instr: Instruction) -> bool {
    matches!(
        instr,
        Instruction::FcvtSW { .. } | Instruction::FcvtSWu { .. } | Instruction::FmvWX { .. }
    )
}

/// rd is an integer register (compares, fcvt.w[u].s, fmv.x.w, fclass.s).
pub fn writes_int(instr: Instruction) -> bool {
    matches!(
        instr,
        Instruction::FeqS { .. }
            | Instruction::FltS { .. }
            | Instruction::FleS { .. }
            | Instruction::FcvtWS { .. }
            | Instruction::FcvtWuS { .. }
            | Instruction::FmvXW { .. }
            | Instruction::FclassS { .. }
    )
}

/// Compute one OP-FP or R4 instruction from its operand bits. `frm` is the
/// dynamic rounding mode (`fcsr[7:5]`).
pub fn execute(instr: Instruction, rs1: u32, rs2: u32, rs3: u32, frm: u8) -> FpResult {
    use Instruction::*;
    let rne = Rm::Rne;
    match instr {
        FaddS { .. } => add(rs1, rs2, rne),
        FsubS { .. } => sub(rs1, rs2, rne),
        FmulS { .. } => mul(rs1, rs2, rne),
        FdivS { .. } => div(rs1, rs2, rne),
        FsqrtS { .. } => sqrt(rs1, rne),
        FminS { .. } => min(rs1, rs2),
        FmaxS { .. } => max(rs1, rs2),
        FsgnjS { .. } => ok((rs1 & !SIGN) | (rs2 & SIGN)),
        FsgnjnS { .. } => ok((rs1 & !SIGN) | (!rs2 & SIGN)),
        FsgnjxS { .. } => ok(rs1 ^ (rs2 & SIGN)),
        FeqS { .. } => feq(rs1, rs2),
        FltS { .. } => flt(rs1, rs2),
        FleS { .. } => fle(rs1, rs2),
        FcvtWS { rm, .. } => to_int(rs1, true, Rm::resolve(rm, frm)),
        FcvtWuS { rm, .. } => to_int(rs1, false, Rm::resolve(rm, frm)),
        FcvtSW { .. } => from_int(rs1, true, rne),
        FcvtSWu { .. } => from_int(rs1, false, rne),
        FmvXW { .. } | FmvWX { .. } => ok(rs1),
        FclassS { .. } => ok(classify(rs1)),
        FmaddS { .. } => fma(rs1, rs2, rs3, false, false, rne),
        FmsubS { .. } => fma(rs1, rs2, rs3, false, true, rne),
        FnmsubS { .. } => fma(rs1, rs2, rs3, true, false, rne),
        FnmaddS { .. } => fma(rs1, rs2, rs3, true, true, rne),
        other => unreachable!("not an F compute instruction: {other:?}"),
    }
}

/// Read one of the floating-point CSRs out of `fcsr`.
pub fn csr_read(fcsr: u32, csr: u16) -> u32 {
    match csr {
        CSR_FFLAGS => fcsr & 0x1F,
        CSR_FRM => (fcsr >> 5) & 0x7,
        _ => fcsr & 0xFF,
    }
}

/// Write one of the floating-point CSRs; returns the new `fcsr`.
pub fn csr_write(fcsr: u32, csr: u16, val: u32) -> u32 {
    match csr {
        CSR_FFLAGS => (fcsr & !0x1F) | (val & 0x1F),
        CSR_FRM => (fcsr & !0xE0) | ((val & 0x7) << 5),
        _ => val & 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RNE: Rm = Rm::Rne;

    /// Deterministic xorshift, so failures reproduce.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 16) as u32
        }
        /// Random bits, biased toward edge exponents and small mantissas.
        fn operand(&mut self) -> u32 {
            let r = self.next();
            match self.next() % 8 {
                0 => r & 0x807F_FFFF,                              // zero or subnormal
                1 => (r & 0x80FF_FFFF) | 0x7F00_0000,              // near overflow, inf, NaN
                2 => (r & 0x8000_000F) | ((r >> 8) & 0x7F80_0000), // short mantissa
                3 => (r & 0x817F_FFFF) | 0x0080_0000,              // near underflow
                _ => r,
            }
        }
    }

    fn same(host: f32, ours: FpResult) -> bool {
        if host.is_nan() {
            ours.bits == CANONICAL_NAN
        } else {
            host.to_bits() == ours.bits
        }
    }

    fn inexact_add(a: f32, b: f32, s: f32) -> bool {
        // TwoSum: the exact rounding error of a + b (valid without overflow).
        let bb = s - a;
        let err = (a - (s - bb)) + (b - bb);
        err != 0.0
    }

    #[test]
    fn arithmetic_matches_the_host_fpu() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..400_000 {
            let (a, b, c) = (rng.operand(), rng.operand(), rng.operand());
            let (x, y, z) = (f32::from_bits(a), f32::from_bits(b), f32::from_bits(c));
            let s = add(a, b, RNE);
            assert!(same(x + y, s), "add {a:08x} {b:08x}");
            if (x + y).is_finite() && !x.is_nan() && !y.is_nan() {
                assert_eq!(
                    s.flags & NX != 0,
                    inexact_add(x, y, x + y),
                    "add NX {a:08x} {b:08x}"
                );
            }
            assert!(same(x - y, sub(a, b, RNE)), "sub {a:08x} {b:08x}");
            let p = mul(a, b, RNE);
            assert!(same(x * y, p), "mul {a:08x} {b:08x}");
            if (x * y).is_finite() && x.is_finite() && y.is_finite() {
                let exact = x as f64 * y as f64; // 48 bits: exact in f64
                assert_eq!(
                    p.flags & NX == 0,
                    (x * y) as f64 == exact,
                    "mul NX {a:08x} {b:08x}"
                );
                // Tiny after rounding to 24 bits with an unbounded exponent;
                // the scaling keeps that rounding inside the normal range.
                let tiny = ((exact * 2f64.powi(64)) as f32).abs() < 2f32.powi(-62);
                let uf = p.flags & NX != 0 && tiny;
                assert_eq!(p.flags & UF != 0, uf, "mul UF {a:08x} {b:08x}");
            }
            let q = div(a, b, RNE);
            assert!(same(x / y, q), "div {a:08x} {b:08x}");
            if (x / y).is_finite() && x.is_finite() && y.is_finite() && y != 0.0 {
                let exact = (x / y) as f64 * y as f64 == x as f64;
                assert_eq!(q.flags & NX == 0, exact, "div NX {a:08x} {b:08x}");
            }
            let r = sqrt(a, RNE);
            assert!(same(x.sqrt(), r), "sqrt {a:08x}");
            assert!(
                same(x.mul_add(y, z), fma(a, b, c, false, false, RNE)),
                "fma {a:08x} {b:08x} {c:08x}"
            );
        }
    }

    fn next_up(b: u32) -> u32 {
        if is_zero(b) {
            1
        } else if sign(b) {
            b - 1
        } else {
            b + 1
        }
    }

    fn next_down(b: u32) -> u32 {
        if is_zero(b) {
            SIGN | 1
        } else if sign(b) {
            b + 1
        } else {
            b - 1
        }
    }

    /// Expected result in a directed mode, from the RNE result `r` and the
    /// sign of the rounding error `err = exact - r`.
    fn directed(r: u32, err: f64, rm: Rm) -> u32 {
        let toward_zero = |r: u32| if sign(r) { next_up(r) } else { next_down(r) };
        match rm {
            _ if err == 0.0 => r,
            Rm::Rup => {
                if err > 0.0 {
                    next_up(r)
                } else {
                    r
                }
            }
            Rm::Rdn => {
                if err < 0.0 {
                    next_down(r)
                } else {
                    r
                }
            }
            Rm::Rtz => {
                if (err < 0.0) != sign(r) && !is_zero(r) {
                    toward_zero(r)
                } else {
                    r
                }
            }
            _ => r,
        }
    }

    #[test]
    fn directed_rounding_matches_neighbors() {
        let mut rng = Rng(0xD1B5_4A32_D192_ED03);
        for _ in 0..200_000 {
            let (a, b) = (rng.operand(), rng.operand());
            let (x, y) = (f32::from_bits(a), f32::from_bits(b));
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let s = x + y;
            if s.is_finite() {
                // TwoSum gives the exact error of the rounded sum.
                let bb = s - x;
                let err = ((x - (s - bb)) + (y - bb)) as f64;
                for rm in [Rm::Rtz, Rm::Rdn, Rm::Rup] {
                    let mut want = directed(s.to_bits(), err, rm);
                    if s == 0.0 && err == 0.0 {
                        // Exact zero: keep a common sign, else +0 (-0 in RDN).
                        let neg = if is_zero(a) && is_zero(b) && sign(a) == sign(b) {
                            sign(a)
                        } else {
                            rm == Rm::Rdn
                        };
                        want = signed(0, neg);
                    }
                    assert_eq!(add(a, b, rm).bits, want, "add {rm:?} {a:08x} {b:08x}");
                }
            }
            let p = x * y;
            if p.is_finite() {
                let err = x as f64 * y as f64 - p as f64;
                for rm in [Rm::Rtz, Rm::Rdn, Rm::Rup] {
                    let want = directed(p.to_bits(), err, rm);
                    assert_eq!(mul(a, b, rm).bits, want, "mul {rm:?} {a:08x} {b:08x}");
                }
                // RMM differs from RNE only on an exact tie.
                let other = if err > 0.0 {
                    next_up(p.to_bits())
                } else {
                    next_down(p.to_bits())
                };
                let tie = err != 0.0
                    && (f32::from_bits(other) as f64 - p as f64).abs() == 2.0 * err.abs();
                let away = if f32::from_bits(other).abs() > p.abs() {
                    other
                } else {
                    p.to_bits()
                };
                let want = if tie { away } else { p.to_bits() };
                assert_eq!(mul(a, b, Rm::Rmm).bits, want, "mul RMM {a:08x} {b:08x}");
            }
        }
    }

    #[test]
    fn flags_on_special_cases() {
        let one = 1.0f32.to_bits();
        let zero = 0u32;
        let inf = INF;
        let snan = 0x7F80_0001;
        assert_eq!(
            div(one, zero, RNE),
            FpResult {
                bits: INF,
                flags: DZ
            }
        );
        assert_eq!(div(zero, zero, RNE), nan(NV));
        assert_eq!(sub(inf, inf, RNE), nan(NV));
        assert_eq!(mul(inf, zero, RNE), nan(NV));
        assert_eq!(sqrt(0xBF80_0000, RNE), nan(NV)); // sqrt(-1)
        assert_eq!(add(snan, one, RNE), nan(NV));
        assert_eq!(add(CANONICAL_NAN, one, RNE), nan(0));
        assert_eq!(
            mul(MAX_FINITE, 2.0f32.to_bits(), RNE),
            FpResult {
                bits: INF,
                flags: OF | NX
            }
        );
        // 2^-126 * 0.5 = 2^-127: exact subnormal, so no UF.
        assert_eq!(mul(0x0080_0000, 0.5f32.to_bits(), RNE).flags, 0);
        // 2^-149 * 0.5 rounds to 0: tiny and inexact.
        assert_eq!(
            mul(1, 0.5f32.to_bits(), RNE),
            FpResult {
                bits: 0,
                flags: UF | NX
            }
        );
        assert_eq!(fma(inf, zero, CANONICAL_NAN, false, false, RNE), nan(NV));
        // fmin/fmax pick the number; a signaling NaN still raises NV.
        assert_eq!(min(CANONICAL_NAN, one), ok(one));
        assert_eq!(
            max(snan, one),
            FpResult {
                bits: one,
                flags: NV
            }
        );
        assert_eq!(min(CANONICAL_NAN, CANONICAL_NAN), ok(CANONICAL_NAN));
        assert_eq!(min(0x8000_0000, 0), ok(0x8000_0000));
        assert_eq!(max(0x8000_0000, 0), ok(0));
        // Quiet vs signaling compares.
        assert_eq!(feq(CANONICAL_NAN, one), ok(0));
        assert_eq!(feq(snan, one), FpResult { bits: 0, flags: NV });
        assert_eq!(flt(CANONICAL_NAN, one), FpResult { bits: 0, flags: NV });
    }

    #[test]
    fn conversions_saturate_and_round() {
        let f = |x: f32| x.to_bits();
        assert_eq!(
            to_int(CANONICAL_NAN, true, RNE),
            FpResult {
                bits: 0x7FFF_FFFF,
                flags: NV
            }
        );
        assert_eq!(
            to_int(CANONICAL_NAN, false, RNE),
            FpResult {
                bits: 0xFFFF_FFFF,
                flags: NV
            }
        );
        assert_eq!(to_int(f(-1.0), false, RNE), FpResult { bits: 0, flags: NV });
        assert_eq!(
            to_int(f(-0.5), false, Rm::Rtz),
            FpResult { bits: 0, flags: NX }
        );
        assert_eq!(
            to_int(f(-1.1), true, Rm::Rtz),
            FpResult {
                bits: -1i32 as u32,
                flags: NX
            }
        );
        assert_eq!(to_int(f(2.5), true, RNE), FpResult { bits: 2, flags: NX });
        assert_eq!(
            to_int(f(2.5), true, Rm::Rmm),
            FpResult { bits: 3, flags: NX }
        );
        assert_eq!(
            to_int(f(-2.5), true, Rm::Rdn),
            FpResult {
                bits: -3i32 as u32,
                flags: NX
            }
        );
        assert_eq!(
            to_int(f(2.1), true, Rm::Rup),
            FpResult { bits: 3, flags: NX }
        );
        assert_eq!(to_int(f(-2147483648.0), true, RNE), ok(0x8000_0000));
        assert_eq!(
            to_int(f(2147483648.0), true, RNE),
            FpResult {
                bits: 0x7FFF_FFFF,
                flags: NV
            }
        );
        assert_eq!(
            from_int(-2i32 as u32, false, RNE),
            FpResult {
                bits: f(4294967296.0),
                flags: NX
            }
        );
        assert_eq!(from_int(-2i32 as u32, true, RNE), ok(f(-2.0)));
        assert_eq!(
            from_int(16_777_217, true, RNE),
            FpResult {
                bits: f(16_777_216.0),
                flags: NX
            }
        );
    }

    #[test]
    fn classify_uses_the_quiet_bit() {
        assert_eq!(classify(0xFF80_0001), 1 << 8); // negative signaling NaN
        assert_eq!(classify(0x7FC0_0000), 1 << 9);
        assert_eq!(classify(0xFFC0_0000), 1 << 9); // negative quiet NaN
        assert_eq!(classify(0x7F80_0001), 1 << 8);
        assert_eq!(classify(0x8000_0001), 1 << 2);
        assert_eq!(classify(0xFF80_0000), 1 << 0);
    }

    #[test]
    fn csr_views_share_one_register() {
        // The sequence from riscv-tests rv64uf/move.S.
        let mut fcsr = csr_write(0, CSR_FCSR, 1);
        fcsr = csr_write(fcsr, CSR_FCSR, 0x1234);
        assert_eq!(csr_read(fcsr, CSR_FCSR), 0x34);
        assert_eq!(csr_read(fcsr, CSR_FFLAGS), 0x14);
        assert_eq!(csr_read(fcsr, CSR_FRM), 1);
        fcsr = csr_write(fcsr, CSR_FRM, 2);
        assert_eq!(fcsr, 0x54);
    }
}
