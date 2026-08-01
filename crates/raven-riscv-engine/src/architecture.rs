use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureCapabilities {
    pub elf: bool,
    pub cache: bool,
    pub virtual_memory: bool,
    pub jit: bool,
    pub pipeline: bool,
    pub multicore: bool,
    pub floating_point: bool,
    pub guided_learning: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchitectureDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source_extension: &'static str,
    pub address_bits: u8,
    pub instruction_alignment: u8,
    pub endianness: Endianness,
    pub capabilities: ArchitectureCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Zero-based source line, when the error belongs to a source line.
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    pub fn new(line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {}: {}", line + 1, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSegment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub executable: bool,
    pub writable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZeroFill {
    pub address: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub comments: HashMap<u64, String>,
    pub block_comments: HashMap<u64, String>,
    pub labels: HashMap<u64, Vec<String>>,
    pub label_to_line: HashMap<String, usize>,
    pub line_addresses: HashMap<usize, u64>,
    pub explicit_halts: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramImage {
    pub architecture: String,
    pub entry: u64,
    pub segments: Vec<ProgramSegment>,
    pub zero_fill: Vec<ZeroFill>,
    pub source_map: SourceMap,
}

impl ProgramImage {
    pub fn executable_bytes(&self) -> Option<&[u8]> {
        self.segments
            .iter()
            .find(|s| s.executable)
            .map(|s| s.bytes.as_slice())
    }

    /// Serialize the architecture-neutral FALC v2 container.
    pub fn to_falc_v2(&self) -> Result<Vec<u8>, MachineError> {
        let arch = self.architecture.as_bytes();
        let arch_len = u16::try_from(arch.len())
            .map_err(|_| MachineError::new("FALC architecture ID is too long"))?;
        let segment_count = u32::try_from(self.segments.len())
            .map_err(|_| MachineError::new("too many FALC segments"))?;
        let fill_count = u32::try_from(self.zero_fill.len())
            .map_err(|_| MachineError::new("too many FALC zero-fill regions"))?;
        let mut out = Vec::new();
        out.extend_from_slice(b"FALC");
        out.extend_from_slice(&u32::MAX.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&arch_len.to_le_bytes());
        out.extend_from_slice(&self.entry.to_le_bytes());
        out.extend_from_slice(&segment_count.to_le_bytes());
        out.extend_from_slice(&fill_count.to_le_bytes());
        out.extend_from_slice(arch);
        for segment in &self.segments {
            out.extend_from_slice(&segment.address.to_le_bytes());
            out.extend_from_slice(&(segment.bytes.len() as u64).to_le_bytes());
            out.push(u8::from(segment.executable) | (u8::from(segment.writable) << 1));
            out.extend_from_slice(&[0; 7]);
            out.extend_from_slice(&segment.bytes);
        }
        for fill in &self.zero_fill {
            out.extend_from_slice(&fill.address.to_le_bytes());
            out.extend_from_slice(&fill.size.to_le_bytes());
        }
        Ok(out)
    }

    /// Read FALC v2, or map the legacy RV32-only FALC v1 layout to an image.
    pub fn from_falc(bytes: &[u8]) -> Result<Self, MachineError> {
        if bytes.len() < 16 || &bytes[..4] != b"FALC" {
            return Err(MachineError::new("not a FALC container"));
        }
        let marker = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if marker != u32::MAX {
            let text_size = marker as usize;
            let data_size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let bss_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as u64;
            let payload_size = text_size
                .checked_add(data_size)
                .ok_or_else(|| MachineError::new("invalid FALC v1 payload size"))?;
            let end = 16usize
                .checked_add(payload_size)
                .ok_or_else(|| MachineError::new("invalid FALC v1 payload size"))?;
            if bytes.len() < end {
                return Err(MachineError::new("truncated FALC v1 container"));
            }
            let data_base = ((text_size as u64) + 3) & !3;
            let mut segments = vec![ProgramSegment {
                address: 0,
                bytes: bytes[16..16 + text_size].to_vec(),
                executable: true,
                writable: false,
            }];
            if data_size > 0 {
                segments.push(ProgramSegment {
                    address: data_base,
                    bytes: bytes[16 + text_size..16 + text_size + data_size].to_vec(),
                    executable: false,
                    writable: true,
                });
            }
            return Ok(Self {
                architecture: "riscv32".into(),
                entry: 0,
                segments,
                zero_fill: (bss_size > 0)
                    .then_some(ZeroFill {
                        address: data_base + data_size as u64,
                        size: bss_size,
                    })
                    .into_iter()
                    .collect(),
                source_map: SourceMap::default(),
            });
        }
        if bytes.len() < 28 {
            return Err(MachineError::new("truncated FALC v2 header"));
        }
        let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        if version != 2 {
            return Err(MachineError::new(format!(
                "unsupported FALC version {version}"
            )));
        }
        let arch_len = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
        let entry = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let segment_count = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        let fill_count = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
        let mut cursor = 28usize;
        let arch_end = cursor
            .checked_add(arch_len)
            .ok_or_else(|| MachineError::new("invalid FALC architecture length"))?;
        let architecture = std::str::from_utf8(
            bytes
                .get(cursor..arch_end)
                .ok_or_else(|| MachineError::new("truncated FALC architecture ID"))?,
        )
        .map_err(|_| MachineError::new("FALC architecture ID is not UTF-8"))?
        .to_string();
        cursor = arch_end;
        let mut segments = Vec::with_capacity(segment_count);
        for _ in 0..segment_count {
            let header_end = cursor
                .checked_add(24)
                .ok_or_else(|| MachineError::new("invalid FALC segment header"))?;
            let header = bytes
                .get(cursor..header_end)
                .ok_or_else(|| MachineError::new("truncated FALC segment header"))?;
            let address = u64::from_le_bytes(header[0..8].try_into().unwrap());
            let len = usize::try_from(u64::from_le_bytes(header[8..16].try_into().unwrap()))
                .map_err(|_| MachineError::new("FALC segment is too large"))?;
            let flags = header[16];
            cursor = header_end;
            let end = cursor
                .checked_add(len)
                .ok_or_else(|| MachineError::new("invalid FALC segment length"))?;
            let payload = bytes
                .get(cursor..end)
                .ok_or_else(|| MachineError::new("truncated FALC segment"))?
                .to_vec();
            cursor = end;
            segments.push(ProgramSegment {
                address,
                bytes: payload,
                executable: flags & 1 != 0,
                writable: flags & 2 != 0,
            });
        }
        let mut zero_fill = Vec::with_capacity(fill_count);
        for _ in 0..fill_count {
            let fill_end = cursor
                .checked_add(16)
                .ok_or_else(|| MachineError::new("invalid FALC zero-fill"))?;
            let fill = bytes
                .get(cursor..fill_end)
                .ok_or_else(|| MachineError::new("truncated FALC zero-fill"))?;
            zero_fill.push(ZeroFill {
                address: u64::from_le_bytes(fill[0..8].try_into().unwrap()),
                size: u64::from_le_bytes(fill[8..16].try_into().unwrap()),
            });
            cursor = fill_end;
        }
        if cursor != bytes.len() {
            return Err(MachineError::new("trailing bytes in FALC v2 container"));
        }
        Ok(Self {
            architecture,
            entry,
            segments,
            zero_fill,
            source_map: SourceMap::default(),
        })
    }
}

pub trait Assembler: Send + Sync {
    fn architecture_id(&self) -> &'static str;
    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterValue {
    pub name: String,
    pub value: u64,
    pub bits: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineState {
    Ready,
    Running,
    AwaitingInput,
    Halted,
    Exited(i32),
    Faulted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineSnapshot {
    pub architecture: &'static str,
    pub pc: u64,
    pub registers: Vec<RegisterValue>,
    pub state: MachineState,
    pub instructions: u64,
    pub stdout: Vec<u8>,
    pub fault: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Stepped,
    AwaitingInput,
    Halted,
    Exited(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError(String);

impl MachineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MachineError {}

pub trait Machine: Send {
    fn architecture_id(&self) -> &'static str;
    fn reset(&mut self);
    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError>;
    fn step(&mut self) -> Result<StepOutcome, MachineError>;
    fn snapshot(&self) -> MachineSnapshot;
    fn read_memory(&self, address: u64, bytes: usize) -> Result<Vec<u8>, MachineError>;
    fn write_memory(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError>;
    fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError>;
    fn push_input(&mut self, line: &str);

    fn run(&mut self, max_steps: u64) -> Result<StepOutcome, MachineError> {
        for _ in 0..max_steps {
            let outcome = self.step()?;
            if outcome != StepOutcome::Stepped {
                return Ok(outcome);
            }
        }
        Err(MachineError::new(format!(
            "execution limit of {max_steps} steps reached"
        )))
    }
}

pub trait Architecture: Send + Sync {
    fn descriptor(&self) -> &'static ArchitectureDescriptor;
    fn assembler(&self) -> &'static dyn Assembler;
    fn default_source(&self) -> &'static str;
    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError>;
}

#[derive(Default, Clone)]
pub struct ArchitectureRegistry {
    entries: HashMap<&'static str, Arc<dyn Architecture>>,
}

impl ArchitectureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(crate::architectures::riscv32::architecture());
        registry.register(crate::architectures::toy16::architecture());
        registry
    }

    pub fn register(
        &mut self,
        architecture: Arc<dyn Architecture>,
    ) -> Option<Arc<dyn Architecture>> {
        self.entries
            .insert(architecture.descriptor().id, architecture)
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Architecture>> {
        self.entries.get(id).cloned()
    }

    pub fn architectures(&self) -> Vec<Arc<dyn Architecture>> {
        let mut values: Vec<_> = self.entries.values().cloned().collect();
        values.sort_by_key(|a| a.descriptor().id);
        values
    }
}
