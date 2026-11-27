use object::elf::{EF_RISCV_RVC, EM_RISCV, FileFlags, PF_W, PF_X, PT_LOAD, ProgramFlags};
use object::read::elf::{ElfFile32, FileHeader, ProgramHeader};
use object::{LittleEndian, Object, ObjectSymbol, SymbolKind};
use rv_abi::{RAM_END, TEXT_BASE};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ElfError {
    #[error("failed to parse ELF32 file: {0}")]
    Parse(String),
    #[error("unsupported ELF: expected 32-bit little-endian RISC-V (EM_RISCV)")]
    UnsupportedArch,
    #[error("ELF contains compressed RVC instructions (EF_RISCV_RVC), which are unsupported in v1")]
    CompressedExtensionUnsupported,
    #[error("PT_LOAD segment at 0x{vaddr:08x}..0x{end:08x} exceeds guest RAM limit 0x{RAM_END:08x}")]
    SegmentOutOfBounds { vaddr: u32, end: u32 },
    #[error("ELF entry point 0x{0:08x} is not inside any executable PT_LOAD segment")]
    InvalidEntryPoint(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSegment {
    pub vaddr: u32,
    pub memsz: u32,
    pub data: Vec<u8>,
    pub is_exec: bool,
    pub is_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElfSymbol {
    pub name: String,
    pub addr: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedElf {
    pub entry: u32,
    pub segments: Vec<LoadSegment>,
    pub symbols: Vec<ElfSymbol>,
    pub initial_brk: u32,
}

impl LoadedElf {
    /// Parse an ELF32 little-endian RISC-V executable from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        let elf = ElfFile32::<LittleEndian>::parse(bytes)
            .map_err(|e| ElfError::Parse(e.to_string()))?;

        let header = elf.elf_header();
        if header.e_machine(LittleEndian) != EM_RISCV {
            return Err(ElfError::UnsupportedArch);
        }

        let e_flags = header.e_flags(LittleEndian);
        if (e_flags & EF_RISCV_RVC) != FileFlags(0) {
            return Err(ElfError::CompressedExtensionUnsupported);
        }

        let entry = header.e_entry(LittleEndian);
        let mut segments = Vec::new();
        let mut max_end = TEXT_BASE;

        for phdr in elf.elf_program_headers() {
            if phdr.p_type(LittleEndian) != PT_LOAD {
                continue;
            }
            let vaddr = phdr.p_vaddr(LittleEndian);
            let memsz = phdr.p_memsz(LittleEndian);
            let filesz = phdr.p_filesz(LittleEndian);
            let end = vaddr.saturating_add(memsz);
            if end > RAM_END {
                return Err(ElfError::SegmentOutOfBounds { vaddr, end });
            }
            if memsz == 0 {
                continue;
            }
            let data = phdr
                .data(LittleEndian, bytes)
                .map_err(|_| ElfError::Parse("invalid PT_LOAD file offset/size".into()))?;
            let copy_len = (filesz as usize).min(data.len()).min(memsz as usize);
            let flags = phdr.p_flags(LittleEndian);
            segments.push(LoadSegment {
                vaddr,
                memsz,
                data: data[..copy_len].to_vec(),
                is_exec: (flags & PF_X) != ProgramFlags(0),
                is_write: (flags & PF_W) != ProgramFlags(0),
            });
            if end > max_end {
                max_end = end;
            }
        }

        let mut symbols = Vec::new();
        for sym in elf.symbols() {
            if sym.kind() == SymbolKind::Text && sym.address() != 0 {
                if let Ok(name) = sym.name() {
                    if !name.is_empty() {
                        symbols.push(ElfSymbol {
                            name: name.to_string(),
                            addr: sym.address() as u32,
                            size: sym.size() as u32,
                        });
                    }
                }
            }
        }
        symbols.sort_by_key(|s| s.addr);
        symbols.dedup_by(|a, b| a.addr == b.addr && a.name == b.name);

        let initial_brk = (max_end + 15) & !15;
        let loaded = Self {
            entry,
            segments,
            symbols,
            initial_brk,
        };

        if !loaded.is_exec_addr(entry) {
            return Err(ElfError::InvalidEntryPoint(entry));
        }

        Ok(loaded)
    }

    pub fn is_exec_addr(&self, addr: u32) -> bool {
        (addr & 3) == 0
            && self
                .segments
                .iter()
                .any(|s| s.is_exec && addr >= s.vaddr && addr.saturating_add(4) <= s.vaddr.saturating_add(s.memsz))
    }

    pub fn read_u32(&self, addr: u32) -> Option<u32> {
        for seg in &self.segments {
            if addr >= seg.vaddr && addr.saturating_add(4) <= seg.vaddr.saturating_add(seg.memsz) {
                let off = (addr - seg.vaddr) as usize;
                let mut buf = [0u8; 4];
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = seg.data.get(off + i).copied().unwrap_or(0);
                }
                return Some(u32::from_le_bytes(buf));
            }
        }
        None
    }

    pub fn symbol_name(&self, addr: u32) -> Option<&str> {
        self.symbols
            .binary_search_by_key(&addr, |s| s.addr)
            .ok()
            .map(|idx| self.symbols[idx].name.as_str())
    }
}
