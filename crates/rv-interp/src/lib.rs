//! Spec-accurate RV32IMF reference interpreter for differential testing and browser comparison.

use rv_abi::{
    FB_BASE, FB_MAX_SIZE, HEAP_LIMIT, HostEnv, LINEAR_MEM_BYTES, STACK_TOP, SYS_CLOCK_MS,
    SYS_EXIT, SYS_POLL_INPUT, SYS_PRESENT_FRAME, SYS_SBRK, SYS_WRITE, write_mmio_input,
};
use rv_decode::{DecodeError, FReg, Instruction, LoadedElf, Reg, decode_instruction};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InterpError {
    #[error("instruction fetch out of bounds at 0x{0:08x}")]
    FetchOutOfBounds(u32),
    #[error("memory access out of bounds at 0x{addr:08x} (size {size})")]
    MemoryOutOfBounds { addr: u32, size: usize },
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("ebreak encountered at 0x{0:08x}")]
    Ebreak(u32),
    #[error("unsupported syscall {0}")]
    UnsupportedSyscall(u32),
}

#[derive(Debug, Clone)]
pub struct CpuState {
    pub x: [u32; 32],
    pub f: [f32; 32],
    pub fcsr: u32,
    pub pc: u32,
    pub instret: u64,
    pub halted: bool,
    pub exit_code: i32,
}

impl Default for CpuState {
    fn default() -> Self {
        let mut x = [0u32; 32];
        x[Reg::SP.index()] = STACK_TOP;
        Self {
            x,
            f: [0.0; 32],
            fcsr: 0,
            pc: 0,
            instret: 0,
            halted: false,
            exit_code: 0,
        }
    }
}

pub struct Interpreter {
    pub cpu: CpuState,
    pub mem: Vec<u8>,
    pub heap_brk: u32,
    pub initial_brk: u32,
    pub fb_width: u32,
    pub fb_height: u32,
    pub frame_count: u32,
}

impl Interpreter {
    pub fn from_elf(elf: &LoadedElf) -> Self {
        let mut mem = vec![0u8; LINEAR_MEM_BYTES];
        for seg in &elf.segments {
            let start = seg.vaddr as usize;
            let copy_len = seg.data.len().min(LINEAR_MEM_BYTES.saturating_sub(start));
            mem[start..start + copy_len].copy_from_slice(&seg.data[..copy_len]);
        }
        let mut cpu = CpuState::default();
        cpu.pc = elf.entry;
        Self {
            cpu,
            mem,
            heap_brk: elf.initial_brk,
            initial_brk: elf.initial_brk,
            fb_width: rv_abi::DEFAULT_FB_WIDTH,
            fb_height: rv_abi::DEFAULT_FB_HEIGHT,
            frame_count: 0,
        }
    }

    #[inline]
    pub fn reg(&self, r: Reg) -> u32 {
        if r.0 == 0 { 0 } else { self.cpu.x[r.index()] }
    }

    #[inline]
    pub fn set_reg(&mut self, r: Reg, val: u32) {
        if r.0 != 0 {
            self.cpu.x[r.index()] = val;
        }
    }

    #[inline]
    pub fn freg(&self, r: FReg) -> f32 {
        self.cpu.f[r.index()]
    }

    #[inline]
    pub fn set_freg(&mut self, r: FReg, val: f32) {
        self.cpu.f[r.index()] = val;
    }

    pub fn memory_crc32(&self) -> u32 {
        crc32fast::hash(&self.mem)
    }

    pub fn framebuffer_slice(&self) -> &[u8] {
        let pixels = (self.fb_width as usize) * (self.fb_height as usize) * 4;
        let len = pixels.min(FB_MAX_SIZE as usize);
        let start = FB_BASE as usize;
        &self.mem[start..start + len]
    }

    #[inline]
    fn read_u8(&self, addr: u32) -> Result<u8, InterpError> {
        self.mem
            .get(addr as usize)
            .copied()
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 1 })
    }

    #[inline]
    fn read_u16(&self, addr: u32) -> Result<u16, InterpError> {
        let idx = addr as usize;
        let bytes = self
            .mem
            .get(idx..idx + 2)
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 2 })?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    #[inline]
    fn read_u32(&self, addr: u32) -> Result<u32, InterpError> {
        let idx = addr as usize;
        let bytes = self
            .mem
            .get(idx..idx + 4)
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 4 })?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, val: u8) -> Result<(), InterpError> {
        let slot = self
            .mem
            .get_mut(addr as usize)
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 1 })?;
        *slot = val;
        Ok(())
    }

    #[inline]
    fn write_u16(&mut self, addr: u32, val: u16) -> Result<(), InterpError> {
        let idx = addr as usize;
        let slot = self
            .mem
            .get_mut(idx..idx + 2)
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 2 })?;
        slot.copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    #[inline]
    fn write_u32(&mut self, addr: u32, val: u32) -> Result<(), InterpError> {
        let idx = addr as usize;
        let slot = self
            .mem
            .get_mut(idx..idx + 4)
            .ok_or(InterpError::MemoryOutOfBounds { addr, size: 4 })?;
        slot.copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Execute a single instruction. Returns `true` if `sys_present_frame` was called.
    pub fn step_one<H: HostEnv>(&mut self, host: &mut H) -> Result<bool, InterpError> {
        if self.cpu.halted {
            return Ok(false);
        }

        let pc = self.cpu.pc;
        let word = self
            .read_u32(pc)
            .map_err(|_| InterpError::FetchOutOfBounds(pc))?;
        let inst = decode_instruction(pc, word)?;
        self.cpu.instret = self.cpu.instret.wrapping_add(1);

        let mut next_pc = pc.wrapping_add(4);
        let mut presented = false;

        match inst {
            Instruction::Lui { rd, imm } => self.set_reg(rd, imm as u32),
            Instruction::Auipc { rd, imm } => self.set_reg(rd, pc.wrapping_add(imm as u32)),
            Instruction::Jal { rd, imm } => {
                self.set_reg(rd, next_pc);
                next_pc = pc.wrapping_add(imm as u32);
            }
            Instruction::Jalr { rd, rs1, imm } => {
                // Compute target before writing rd in case rd == rs1.
                let target = self.reg(rs1).wrapping_add(imm as u32) & !1;
                self.set_reg(rd, next_pc);
                next_pc = target;
            }

            Instruction::Beq { rs1, rs2, imm } => {
                if self.reg(rs1) == self.reg(rs2) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }
            Instruction::Bne { rs1, rs2, imm } => {
                if self.reg(rs1) != self.reg(rs2) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }
            Instruction::Blt { rs1, rs2, imm } => {
                if (self.reg(rs1) as i32) < (self.reg(rs2) as i32) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }
            Instruction::Bge { rs1, rs2, imm } => {
                if (self.reg(rs1) as i32) >= (self.reg(rs2) as i32) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }
            Instruction::Bltu { rs1, rs2, imm } => {
                if self.reg(rs1) < self.reg(rs2) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }
            Instruction::Bgeu { rs1, rs2, imm } => {
                if self.reg(rs1) >= self.reg(rs2) {
                    next_pc = pc.wrapping_add(imm as u32);
                }
            }

            Instruction::Lb { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let val = self.read_u8(addr)? as i8 as i32 as u32;
                self.set_reg(rd, val);
            }
            Instruction::Lh { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let val = self.read_u16(addr)? as i16 as i32 as u32;
                self.set_reg(rd, val);
            }
            Instruction::Lw { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let val = self.read_u32(addr)?;
                self.set_reg(rd, val);
            }
            Instruction::Lbu { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let val = self.read_u8(addr)? as u32;
                self.set_reg(rd, val);
            }
            Instruction::Lhu { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let val = self.read_u16(addr)? as u32;
                self.set_reg(rd, val);
            }

            Instruction::Sb { rs1, rs2, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                self.write_u8(addr, self.reg(rs2) as u8)?;
            }
            Instruction::Sh { rs1, rs2, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                self.write_u16(addr, self.reg(rs2) as u16)?;
            }
            Instruction::Sw { rs1, rs2, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                self.write_u32(addr, self.reg(rs2))?;
            }

            Instruction::Addi { rd, rs1, imm } => {
                self.set_reg(rd, self.reg(rs1).wrapping_add(imm as u32));
            }
            Instruction::Slti { rd, rs1, imm } => {
                let v = if (self.reg(rs1) as i32) < imm { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::Sltiu { rd, rs1, imm } => {
                let v = if self.reg(rs1) < (imm as u32) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::Xori { rd, rs1, imm } => {
                self.set_reg(rd, self.reg(rs1) ^ (imm as u32));
            }
            Instruction::Ori { rd, rs1, imm } => {
                self.set_reg(rd, self.reg(rs1) | (imm as u32));
            }
            Instruction::Andi { rd, rs1, imm } => {
                self.set_reg(rd, self.reg(rs1) & (imm as u32));
            }
            Instruction::Slli { rd, rs1, shamt } => {
                self.set_reg(rd, self.reg(rs1).wrapping_shl(shamt as u32));
            }
            Instruction::Srli { rd, rs1, shamt } => {
                self.set_reg(rd, self.reg(rs1).wrapping_shr(shamt as u32));
            }
            Instruction::Srai { rd, rs1, shamt } => {
                self.set_reg(rd, ((self.reg(rs1) as i32).wrapping_shr(shamt as u32)) as u32);
            }

            Instruction::Add { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1).wrapping_add(self.reg(rs2)));
            }
            Instruction::Sub { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1).wrapping_sub(self.reg(rs2)));
            }
            Instruction::Sll { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1).wrapping_shl(self.reg(rs2) & 0x1F));
            }
            Instruction::Slt { rd, rs1, rs2 } => {
                let v = if (self.reg(rs1) as i32) < (self.reg(rs2) as i32) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::Sltu { rd, rs1, rs2 } => {
                let v = if self.reg(rs1) < self.reg(rs2) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::Xor { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1) ^ self.reg(rs2));
            }
            Instruction::Srl { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1).wrapping_shr(self.reg(rs2) & 0x1F));
            }
            Instruction::Sra { rd, rs1, rs2 } => {
                let sh = self.reg(rs2) & 0x1F;
                self.set_reg(rd, ((self.reg(rs1) as i32).wrapping_shr(sh)) as u32);
            }
            Instruction::Or { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1) | self.reg(rs2));
            }
            Instruction::And { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1) & self.reg(rs2));
            }

            // RV32M
            Instruction::Mul { rd, rs1, rs2 } => {
                self.set_reg(rd, self.reg(rs1).wrapping_mul(self.reg(rs2)));
            }
            Instruction::Mulh { rd, rs1, rs2 } => {
                let a = self.reg(rs1) as i32 as i64;
                let b = self.reg(rs2) as i32 as i64;
                self.set_reg(rd, ((a.wrapping_mul(b)) >> 32) as u32);
            }
            Instruction::Mulhsu { rd, rs1, rs2 } => {
                let a = self.reg(rs1) as i32 as i64 as u64;
                let b = self.reg(rs2) as u64;
                self.set_reg(rd, ((a.wrapping_mul(b)) >> 32) as u32);
            }
            Instruction::Mulhu { rd, rs1, rs2 } => {
                let a = self.reg(rs1) as u64;
                let b = self.reg(rs2) as u64;
                self.set_reg(rd, ((a.wrapping_mul(b)) >> 32) as u32);
            }
            Instruction::Div { rd, rs1, rs2 } => {
                let a = self.reg(rs1) as i32;
                let b = self.reg(rs2) as i32;
                let res = if b == 0 {
                    -1
                } else if a == i32::MIN && b == -1 {
                    i32::MIN
                } else {
                    a.wrapping_div(b)
                };
                self.set_reg(rd, res as u32);
            }
            Instruction::Divu { rd, rs1, rs2 } => {
                let a = self.reg(rs1);
                let b = self.reg(rs2);
                let res = if b == 0 { u32::MAX } else { a / b };
                self.set_reg(rd, res);
            }
            Instruction::Rem { rd, rs1, rs2 } => {
                let a = self.reg(rs1) as i32;
                let b = self.reg(rs2) as i32;
                let res = if b == 0 {
                    a
                } else if a == i32::MIN && b == -1 {
                    0
                } else {
                    a.wrapping_rem(b)
                };
                self.set_reg(rd, res as u32);
            }
            Instruction::Remu { rd, rs1, rs2 } => {
                let a = self.reg(rs1);
                let b = self.reg(rs2);
                let res = if b == 0 { a } else { a % b };
                self.set_reg(rd, res);
            }

            // RV32F
            Instruction::Flw { rd, rs1, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                let bits = self.read_u32(addr)?;
                self.set_freg(rd, f32::from_bits(bits));
            }
            Instruction::Fsw { rs1, rs2, imm } => {
                let addr = self.reg(rs1).wrapping_add(imm as u32);
                self.write_u32(addr, self.freg(rs2).to_bits())?;
            }
            Instruction::FmaddS { rd, rs1, rs2, rs3, .. } => {
                let res = self.freg(rs1).mul_add(self.freg(rs2), self.freg(rs3));
                self.set_freg(rd, canon_f32(res));
            }
            Instruction::FmsubS { rd, rs1, rs2, rs3, .. } => {
                let res = self.freg(rs1).mul_add(self.freg(rs2), -self.freg(rs3));
                self.set_freg(rd, canon_f32(res));
            }
            Instruction::FnmsubS { rd, rs1, rs2, rs3, .. } => {
                let res = (-self.freg(rs1)).mul_add(self.freg(rs2), self.freg(rs3));
                self.set_freg(rd, canon_f32(res));
            }
            Instruction::FnmaddS { rd, rs1, rs2, rs3, .. } => {
                let res = (-self.freg(rs1)).mul_add(self.freg(rs2), -self.freg(rs3));
                self.set_freg(rd, canon_f32(res));
            }
            Instruction::FaddS { rd, rs1, rs2, .. } => {
                self.set_freg(rd, canon_f32(self.freg(rs1) + self.freg(rs2)));
            }
            Instruction::FsubS { rd, rs1, rs2, .. } => {
                self.set_freg(rd, canon_f32(self.freg(rs1) - self.freg(rs2)));
            }
            Instruction::FmulS { rd, rs1, rs2, .. } => {
                self.set_freg(rd, canon_f32(self.freg(rs1) * self.freg(rs2)));
            }
            Instruction::FdivS { rd, rs1, rs2, .. } => {
                self.set_freg(rd, canon_f32(self.freg(rs1) / self.freg(rs2)));
            }
            Instruction::FsqrtS { rd, rs1, .. } => {
                self.set_freg(rd, canon_f32(self.freg(rs1).sqrt()));
            }
            Instruction::FsgnjS { rd, rs1, rs2 } => {
                let a = self.freg(rs1).to_bits();
                let b = self.freg(rs2).to_bits();
                self.set_freg(rd, f32::from_bits((a & 0x7FFF_FFFF) | (b & 0x8000_0000)));
            }
            Instruction::FsgnjnS { rd, rs1, rs2 } => {
                let a = self.freg(rs1).to_bits();
                let b = self.freg(rs2).to_bits();
                self.set_freg(rd, f32::from_bits((a & 0x7FFF_FFFF) | ((!b) & 0x8000_0000)));
            }
            Instruction::FsgnjxS { rd, rs1, rs2 } => {
                let a = self.freg(rs1).to_bits();
                let b = self.freg(rs2).to_bits();
                self.set_freg(rd, f32::from_bits(a ^ (b & 0x8000_0000)));
            }
            Instruction::FminS { rd, rs1, rs2 } => {
                self.set_freg(rd, rv_fmin(self.freg(rs1), self.freg(rs2)));
            }
            Instruction::FmaxS { rd, rs1, rs2 } => {
                self.set_freg(rd, rv_fmax(self.freg(rs1), self.freg(rs2)));
            }
            Instruction::FcvtWS { rd, rs1, .. } => {
                self.set_reg(rd, rv_fcvt_w_s(self.freg(rs1)) as u32);
            }
            Instruction::FcvtWuS { rd, rs1, .. } => {
                self.set_reg(rd, rv_fcvt_wu_s(self.freg(rs1)));
            }
            Instruction::FmvXW { rd, rs1 } => {
                self.set_reg(rd, self.freg(rs1).to_bits());
            }
            Instruction::FclassS { rd, rs1 } => {
                self.set_reg(rd, rv_fclass_s(self.freg(rs1)));
            }
            Instruction::FeqS { rd, rs1, rs2 } => {
                let v = if self.freg(rs1) == self.freg(rs2) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::FltS { rd, rs1, rs2 } => {
                let v = if self.freg(rs1) < self.freg(rs2) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::FleS { rd, rs1, rs2 } => {
                let v = if self.freg(rs1) <= self.freg(rs2) { 1 } else { 0 };
                self.set_reg(rd, v);
            }
            Instruction::FcvtSW { rd, rs1, .. } => {
                self.set_freg(rd, canon_f32((self.reg(rs1) as i32) as f32));
            }
            Instruction::FcvtSWu { rd, rs1, .. } => {
                self.set_freg(rd, canon_f32(self.reg(rs1) as f32));
            }
            Instruction::FmvWX { rd, rs1 } => {
                self.set_freg(rd, f32::from_bits(self.reg(rs1)));
            }

            Instruction::Csrrw { rd, rs1, csr } => {
                let old = self.read_csr(csr);
                let val = self.reg(rs1);
                self.write_csr(csr, val);
                self.set_reg(rd, old);
            }
            Instruction::Csrrs { rd, rs1, csr } => {
                let old = self.read_csr(csr);
                if rs1 != Reg::ZERO {
                    self.write_csr(csr, old | self.reg(rs1));
                }
                self.set_reg(rd, old);
            }
            Instruction::Csrrc { rd, rs1, csr } => {
                let old = self.read_csr(csr);
                if rs1 != Reg::ZERO {
                    self.write_csr(csr, old & !self.reg(rs1));
                }
                self.set_reg(rd, old);
            }
            Instruction::Csrrwi { rd, zimm, csr } => {
                let old = self.read_csr(csr);
                self.write_csr(csr, zimm as u32);
                self.set_reg(rd, old);
            }
            Instruction::Csrrsi { rd, zimm, csr } => {
                let old = self.read_csr(csr);
                if zimm != 0 {
                    self.write_csr(csr, old | (zimm as u32));
                }
                self.set_reg(rd, old);
            }
            Instruction::Csrrci { rd, zimm, csr } => {
                let old = self.read_csr(csr);
                if zimm != 0 {
                    self.write_csr(csr, old & !(zimm as u32));
                }
                self.set_reg(rd, old);
            }

            Instruction::Fence => {}
            Instruction::Ebreak => return Err(InterpError::Ebreak(pc)),
            Instruction::Ecall => {
                presented = self.handle_ecall(host)?;
            }
        }

        self.cpu.pc = next_pc;
        self.cpu.x[0] = 0;
        Ok(presented)
    }

    fn handle_ecall<H: HostEnv>(&mut self, host: &mut H) -> Result<bool, InterpError> {
        let id = self.reg(Reg::A7);
        let a0 = self.reg(Reg::A0);
        let a1 = self.reg(Reg::A1);
        let a2 = self.reg(Reg::A2);

        match id {
            SYS_WRITE => {
                let start = a1 as usize;
                let len = a2 as usize;
                let slice = self
                    .mem
                    .get(start..start.saturating_add(len))
                    .ok_or(InterpError::MemoryOutOfBounds {
                        addr: a1,
                        size: len,
                    })?;
                let written = host.write_fd(a0, slice);
                self.set_reg(Reg::A0, written as u32);
                Ok(false)
            }
            SYS_EXIT => {
                let code = a0 as i32;
                self.cpu.halted = true;
                self.cpu.exit_code = code;
                host.on_exit(code);
                Ok(false)
            }
            SYS_CLOCK_MS => {
                let ms = host.clock_ms();
                self.set_reg(Reg::A0, ms);
                Ok(false)
            }
            SYS_PRESENT_FRAME => {
                if a0 > 0 && a0 <= rv_abi::FB_MAX_WIDTH {
                    self.fb_width = a0;
                }
                if a1 > 0 && a1 <= rv_abi::FB_MAX_HEIGHT {
                    self.fb_height = a1;
                }
                self.frame_count = self.frame_count.wrapping_add(1);
                let fb_len = (self.fb_width as usize) * (self.fb_height as usize) * 4;
                let fb_start = FB_BASE as usize;
                let fb_bytes = &self.mem[fb_start..fb_start + fb_len];
                host.present_frame(self.fb_width, self.fb_height, fb_bytes);
                self.set_reg(Reg::A0, 0);
                Ok(true)
            }
            SYS_POLL_INPUT => {
                let input = host.poll_input();
                let ms = host.clock_ms();
                write_mmio_input(
                    &mut self.mem,
                    input,
                    ms,
                    self.fb_width,
                    self.fb_height,
                    self.frame_count,
                );
                self.set_reg(Reg::A0, input.keys_lo);
                Ok(false)
            }
            SYS_SBRK => {
                let incr = a0 as i32;
                let old_brk = self.heap_brk;
                let new_brk = (old_brk as i64).saturating_add(incr as i64);
                if new_brk < (self.initial_brk as i64) || new_brk > (HEAP_LIMIT as i64) {
                    self.set_reg(Reg::A0, (-1i32) as u32);
                } else {
                    self.heap_brk = new_brk as u32;
                    self.set_reg(Reg::A0, old_brk);
                }
                Ok(false)
            }
            _ => Err(InterpError::UnsupportedSyscall(id)),
        }
    }

    /// Run until halted or `max_steps` instructions executed.
    pub fn run<H: HostEnv>(&mut self, host: &mut H, max_steps: u64) -> Result<u64, InterpError> {
        let mut steps = 0;
        while !self.cpu.halted && steps < max_steps {
            self.step_one(host)?;
            steps += 1;
        }
        Ok(steps)
    }

    fn read_csr(&self, csr: u16) -> u32 {
        match csr {
            0x001 => self.cpu.fcsr & 0x1F,
            0x002 => (self.cpu.fcsr >> 5) & 0x7,
            0x003 => self.cpu.fcsr & 0xFF,
            _ => 0,
        }
    }

    fn write_csr(&mut self, csr: u16, val: u32) {
        match csr {
            0x001 => self.cpu.fcsr = (self.cpu.fcsr & !0x1F) | (val & 0x1F),
            0x002 => self.cpu.fcsr = (self.cpu.fcsr & !(0x7 << 5)) | ((val & 0x7) << 5),
            0x003 => self.cpu.fcsr = val & 0xFF,
            _ => {}
        }
    }
}

#[inline]
fn canon_f32(x: f32) -> f32 {
    if x.is_nan() {
        f32::from_bits(0x7FC0_0000)
    } else {
        x
    }
}

#[inline]
fn rv_fmin(a: f32, b: f32) -> f32 {
    if a.is_nan() && b.is_nan() {
        return f32::from_bits(0x7FC0_0000);
    }
    if a.is_nan() {
        return b;
    }
    if b.is_nan() {
        return a;
    }
    if a.to_bits() == 0x8000_0000 && b.to_bits() == 0x0000_0000 {
        return a;
    }
    if b.to_bits() == 0x8000_0000 && a.to_bits() == 0x0000_0000 {
        return b;
    }
    a.min(b)
}

#[inline]
fn rv_fmax(a: f32, b: f32) -> f32 {
    if a.is_nan() && b.is_nan() {
        return f32::from_bits(0x7FC0_0000);
    }
    if a.is_nan() {
        return b;
    }
    if b.is_nan() {
        return a;
    }
    if a.to_bits() == 0x8000_0000 && b.to_bits() == 0x0000_0000 {
        return b;
    }
    if b.to_bits() == 0x8000_0000 && a.to_bits() == 0x0000_0000 {
        return a;
    }
    a.max(b)
}

#[inline]
fn rv_fcvt_w_s(x: f32) -> i32 {
    if x.is_nan() {
        return i32::MAX;
    }
    let r = x.round_ties_even();
    if r >= 2147483648.0 {
        i32::MAX
    } else if r < -2147483648.0 {
        i32::MIN
    } else {
        r as i32
    }
}

#[inline]
fn rv_fcvt_wu_s(x: f32) -> u32 {
    if x.is_nan() {
        return u32::MAX;
    }
    let r = x.round_ties_even();
    if r >= 4294967296.0 {
        u32::MAX
    } else if r < 0.0 {
        0
    } else {
        r as u32
    }
}

#[inline]
fn rv_fclass_s(x: f32) -> u32 {
    let bits = x.to_bits();
    let sign = (bits >> 31) != 0;
    let exp = (bits >> 23) & 0xFF;
    let mant = bits & 0x7F_FFFF;
    if exp == 0xFF {
        if mant == 0 {
            if sign { 1 << 0 } else { 1 << 7 }
        } else if (mant & 0x40_0000) == 0 {
            1 << 8 // signaling NaN
        } else {
            1 << 9 // quiet NaN
        }
    } else if exp == 0 {
        if mant == 0 {
            if sign { 1 << 3 } else { 1 << 4 }
        } else if sign {
            1 << 2
        } else {
            1 << 5
        }
    } else if sign {
        1 << 1
    } else {
        1 << 6
    }
}
