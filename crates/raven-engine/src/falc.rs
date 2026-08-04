//! The FALC container format.
//!
//! `v2` is architecture-neutral: it names its target backend and carries an
//! arbitrary number of segments and zero-fill regions. `v1` is the legacy
//! RV32-only layout and is read-only — it is decoded into the same
//! [`ProgramImage`] so the rest of the engine never has to know which version
//! a file came from.
//!
//! Neither version carries a [`SourceMap`](crate::SourceMap): labels and
//! comments live in the assembler's output, not in the shipped binary, so a
//! decoded image always has an empty source map.

use crate::architecture::{MachineError, ProgramImage, ProgramSegment, ZeroFill};

const MAGIC: &[u8; 4] = b"FALC";
/// Written where v1 keeps its text size; no real v1 program is this large, so
/// it unambiguously marks a v2 container.
const V2_MARKER: u32 = u32::MAX;
const V2_VERSION: u16 = 2;
const V2_HEADER_LEN: usize = 28;
const V1_HEADER_LEN: usize = 16;
const SEGMENT_HEADER_LEN: usize = 24;
const ZERO_FILL_LEN: usize = 16;

const FLAG_EXECUTABLE: u8 = 1;
const FLAG_WRITABLE: u8 = 2;

/// Serialize `image` as a FALC v2 container.
pub(crate) fn encode_v2(image: &ProgramImage) -> Result<Vec<u8>, MachineError> {
    let architecture = image.architecture.as_bytes();
    let architecture_len = u16::try_from(architecture.len())
        .map_err(|_| MachineError::new("FALC architecture ID is too long"))?;
    let segment_count = u32::try_from(image.segments.len())
        .map_err(|_| MachineError::new("too many FALC segments"))?;
    let fill_count = u32::try_from(image.zero_fill.len())
        .map_err(|_| MachineError::new("too many FALC zero-fill regions"))?;

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&V2_MARKER.to_le_bytes());
    out.extend_from_slice(&V2_VERSION.to_le_bytes());
    out.extend_from_slice(&architecture_len.to_le_bytes());
    out.extend_from_slice(&image.entry.to_le_bytes());
    out.extend_from_slice(&segment_count.to_le_bytes());
    out.extend_from_slice(&fill_count.to_le_bytes());
    out.extend_from_slice(architecture);
    for segment in &image.segments {
        let mut flags = 0;
        if segment.executable {
            flags |= FLAG_EXECUTABLE;
        }
        if segment.writable {
            flags |= FLAG_WRITABLE;
        }
        out.extend_from_slice(&segment.address.to_le_bytes());
        out.extend_from_slice(&(segment.bytes.len() as u64).to_le_bytes());
        out.push(flags);
        out.extend_from_slice(&[0; SEGMENT_HEADER_LEN - 17]); // reserved padding
        out.extend_from_slice(&segment.bytes);
    }
    for fill in &image.zero_fill {
        out.extend_from_slice(&fill.address.to_le_bytes());
        out.extend_from_slice(&fill.size.to_le_bytes());
    }
    Ok(out)
}

/// Decode either container version into a [`ProgramImage`].
pub(crate) fn decode(bytes: &[u8]) -> Result<ProgramImage, MachineError> {
    if bytes.len() < V1_HEADER_LEN || !bytes.starts_with(MAGIC) {
        return Err(MachineError::new("not a FALC container"));
    }
    if read_u32(bytes, 4) == V2_MARKER {
        decode_v2(bytes)
    } else {
        decode_v1(bytes)
    }
}

/// Legacy RV32 layout: `FALC | text_size | data_size | bss_size | text | data`,
/// with text at 0 and data placed right after it, 4-byte aligned.
fn decode_v1(bytes: &[u8]) -> Result<ProgramImage, MachineError> {
    let text_size = read_u32(bytes, 4) as usize;
    let data_size = read_u32(bytes, 8) as usize;
    let bss_size = u64::from(read_u32(bytes, 12));
    let end = text_size
        .checked_add(data_size)
        .and_then(|payload| payload.checked_add(V1_HEADER_LEN))
        .ok_or_else(|| MachineError::new("invalid FALC v1 payload size"))?;
    if bytes.len() < end {
        return Err(MachineError::new("truncated FALC v1 container"));
    }

    let text_end = V1_HEADER_LEN + text_size;
    let data_base = ((text_size as u64) + 3) & !3;
    let mut segments = vec![ProgramSegment {
        address: 0,
        bytes: bytes[V1_HEADER_LEN..text_end].to_vec(),
        executable: true,
        writable: false,
    }];
    if data_size > 0 {
        segments.push(ProgramSegment {
            address: data_base,
            bytes: bytes[text_end..text_end + data_size].to_vec(),
            executable: false,
            writable: true,
        });
    }
    Ok(ProgramImage {
        architecture: crate::architectures::riscv32::ID.into(),
        entry: 0,
        segments,
        zero_fill: Vec::from_iter((bss_size > 0).then_some(ZeroFill {
            address: data_base + data_size as u64,
            size: bss_size,
        })),
        source_map: Default::default(),
    })
}

fn decode_v2(bytes: &[u8]) -> Result<ProgramImage, MachineError> {
    if bytes.len() < V2_HEADER_LEN {
        return Err(MachineError::new("truncated FALC v2 header"));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != V2_VERSION {
        return Err(MachineError::new(format!(
            "unsupported FALC version {version}"
        )));
    }
    let architecture_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
    let entry = read_u64(bytes, 12);
    let segment_count = read_u32(bytes, 20) as usize;
    let fill_count = read_u32(bytes, 24) as usize;

    let mut cursor = Cursor::new(bytes, V2_HEADER_LEN);
    let architecture = std::str::from_utf8(cursor.take(architecture_len, "architecture ID")?)
        .map_err(|_| MachineError::new("FALC architecture ID is not UTF-8"))?
        .to_string();

    let mut segments = Vec::with_capacity(segment_count);
    for _ in 0..segment_count {
        let header = cursor.take(SEGMENT_HEADER_LEN, "segment header")?;
        let address = read_u64(header, 0);
        let len = usize::try_from(read_u64(header, 8))
            .map_err(|_| MachineError::new("FALC segment is too large"))?;
        let flags = header[16];
        segments.push(ProgramSegment {
            address,
            bytes: cursor.take(len, "segment")?.to_vec(),
            executable: flags & FLAG_EXECUTABLE != 0,
            writable: flags & FLAG_WRITABLE != 0,
        });
    }

    let mut zero_fill = Vec::with_capacity(fill_count);
    for _ in 0..fill_count {
        let fill = cursor.take(ZERO_FILL_LEN, "zero-fill")?;
        zero_fill.push(ZeroFill {
            address: read_u64(fill, 0),
            size: read_u64(fill, 8),
        });
    }
    if !cursor.is_exhausted() {
        return Err(MachineError::new("trailing bytes in FALC v2 container"));
    }
    Ok(ProgramImage {
        architecture,
        entry,
        segments,
        zero_fill,
        source_map: Default::default(),
    })
}

/// Bounds-checked forward reader over the container body.
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], offset: usize) -> Self {
        Self { bytes, offset }
    }

    fn take(&mut self, len: usize, what: &str) -> Result<&'a [u8], MachineError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| MachineError::new(format!("invalid FALC {what} length")))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| MachineError::new(format!("truncated FALC {what}")))?;
        self.offset = end;
        Ok(slice)
    }

    fn is_exhausted(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

/// Both readers are only ever called on ranges the caller already bounds-checked.
fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
