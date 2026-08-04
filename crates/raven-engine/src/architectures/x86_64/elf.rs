//! ELF64 loader for freestanding/static x86-64 executables.

use super::ID;
use crate::{MachineError, ProgramImage, ProgramSegment, SourceMap, ZeroFill};

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

/// Decode a static/freestanding ELF64 x86-64 executable into Raven's neutral
/// image format.
///
/// Dynamically linked and PIE executables are rejected because Raven does not
/// provide an ELF dynamic linker or relocation engine.
pub fn image_from_elf(bytes: &[u8]) -> Result<ProgramImage, MachineError> {
    let reader = Reader(bytes);
    if reader.slice(0, 4)? != b"\x7FELF" {
        return Err(MachineError::new("not an ELF file"));
    }
    if reader.byte(4)? != 2 {
        return Err(MachineError::new("x86-64 requires an ELF64 file"));
    }
    if reader.byte(5)? != 1 {
        return Err(MachineError::new("x86-64 ELF must be little-endian"));
    }
    if reader.byte(6)? != 1 || reader.u32(20)? != 1 {
        return Err(MachineError::new("unsupported ELF version"));
    }
    if reader.u16(16)? != ET_EXEC {
        return Err(MachineError::new(
            "only static ET_EXEC x86-64 ELF files are supported",
        ));
    }
    if reader.u16(18)? != EM_X86_64 {
        return Err(MachineError::new("ELF machine is not x86-64"));
    }
    if usize::from(reader.u16(52)?) < ELF_HEADER_SIZE {
        return Err(MachineError::new("truncated ELF64 header"));
    }

    let entry = reader.u64(24)?;
    let table = usize::try_from(reader.u64(32)?)
        .map_err(|_| MachineError::new("ELF program-header offset is too large"))?;
    let entry_size = usize::from(reader.u16(54)?);
    let count = usize::from(reader.u16(56)?);
    if entry_size < PROGRAM_HEADER_SIZE || count == 0 {
        return Err(MachineError::new("ELF has no usable program headers"));
    }
    let table_size = entry_size
        .checked_mul(count)
        .ok_or_else(|| MachineError::new("ELF program-header table is too large"))?;
    reader.slice(table, table_size)?;

    let mut headers = Vec::new();
    for index in 0..count {
        let offset = table
            .checked_add(index * entry_size)
            .ok_or_else(|| MachineError::new("ELF program-header offset overflow"))?;
        let kind = reader.u32(offset)?;
        if kind == PT_INTERP {
            return Err(MachineError::new(
                "dynamically linked x86-64 ELF files are not supported",
            ));
        }
        if kind != PT_LOAD {
            continue;
        }
        let flags = reader.u32(offset + 4)?;
        let file_offset = reader.u64(offset + 8)?;
        let address = reader.u64(offset + 16)?;
        let file_size = reader.u64(offset + 32)?;
        let memory_size = reader.u64(offset + 40)?;
        if file_size > memory_size {
            return Err(MachineError::new(format!(
                "ELF PT_LOAD {index} has filesz larger than memsz"
            )));
        }
        address
            .checked_add(memory_size)
            .ok_or_else(|| MachineError::new("ELF segment address overflow"))?;
        let file_offset = usize::try_from(file_offset)
            .map_err(|_| MachineError::new("ELF segment offset is too large"))?;
        let file_size = usize::try_from(file_size)
            .map_err(|_| MachineError::new("ELF segment is too large"))?;
        let data = reader.slice(file_offset, file_size)?.to_vec();
        headers.push(LoadHeader {
            address,
            data,
            memory_size,
            executable: flags & PF_X != 0,
            writable: flags & PF_W != 0,
        });
    }
    if headers.is_empty() {
        return Err(MachineError::new("ELF contains no PT_LOAD segments"));
    }
    if !headers.iter().any(|header| {
        header.executable
            && entry >= header.address
            && entry < header.address.saturating_add(header.memory_size)
    }) {
        return Err(MachineError::new(
            "ELF entry point is outside executable PT_LOAD segments",
        ));
    }
    headers.sort_by_key(|header| header.address);

    let mut segments = Vec::new();
    let mut zero_fill = Vec::new();
    for header in headers {
        let initialized = header.data.len() as u64;
        if !header.data.is_empty() {
            segments.push(ProgramSegment {
                address: header.address,
                bytes: header.data,
                executable: header.executable,
                writable: header.writable,
            });
        }
        if header.memory_size > initialized {
            zero_fill.push(ZeroFill {
                address: header.address + initialized,
                size: header.memory_size - initialized,
            });
        }
    }

    Ok(ProgramImage {
        architecture: ID.into(),
        entry,
        segments,
        zero_fill,
        source_map: SourceMap::default(),
    })
}

struct LoadHeader {
    address: u64,
    data: Vec<u8>,
    memory_size: u64,
    executable: bool,
    writable: bool,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn slice(&self, offset: usize, size: usize) -> Result<&[u8], MachineError> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| MachineError::new("ELF offset overflow"))?;
        self.0
            .get(offset..end)
            .ok_or_else(|| MachineError::new("truncated ELF file"))
    }

    fn byte(&self, offset: usize) -> Result<u8, MachineError> {
        self.0
            .get(offset)
            .copied()
            .ok_or_else(|| MachineError::new("truncated ELF file"))
    }

    fn u16(&self, offset: usize) -> Result<u16, MachineError> {
        Ok(u16::from_le_bytes(
            self.slice(offset, 2)?.try_into().unwrap(),
        ))
    }

    fn u32(&self, offset: usize) -> Result<u32, MachineError> {
        Ok(u32::from_le_bytes(
            self.slice(offset, 4)?.try_into().unwrap(),
        ))
    }

    fn u64(&self, offset: usize) -> Result<u64, MachineError> {
        Ok(u64::from_le_bytes(
            self.slice(offset, 8)?.try_into().unwrap(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Machine, StepOutcome};

    fn elf(machine: u16, kind: u16) -> Vec<u8> {
        let mut bytes = vec![0; 0x101];
        bytes[..4].copy_from_slice(b"\x7FELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[6] = 1;
        bytes[16..18].copy_from_slice(&kind.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());

        bytes[64..68].copy_from_slice(&PT_LOAD.to_le_bytes());
        bytes[68..72].copy_from_slice(&(PF_X | 4).to_le_bytes());
        bytes[72..80].copy_from_slice(&0x100u64.to_le_bytes());
        bytes[80..88].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes[96..104].copy_from_slice(&1u64.to_le_bytes());
        bytes[104..112].copy_from_slice(&9u64.to_le_bytes());
        bytes[112..120].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes[0x100] = 0xF4;
        bytes
    }

    #[test]
    fn maps_load_segments_and_bss() {
        let image = image_from_elf(&elf(EM_X86_64, ET_EXEC)).unwrap();
        assert_eq!(image.entry, 0x1000);
        assert_eq!(image.executable_bytes(), Some([0xF4].as_slice()));
        assert_eq!(
            image.zero_fill[0],
            ZeroFill {
                address: 0x1001,
                size: 8
            }
        );

        let mut machine = super::super::X86_64Machine::new(0x2000);
        machine.load(&image).unwrap();
        assert_eq!(machine.run(1).unwrap(), StepOutcome::Halted);
        assert_eq!(machine.read_memory(0x1001, 8).unwrap(), [0; 8]);
    }

    #[test]
    fn rejects_foreign_and_position_independent_files() {
        assert!(image_from_elf(&elf(3, ET_EXEC)).is_err());
        assert!(image_from_elf(&elf(EM_X86_64, 3)).is_err());
    }
}
