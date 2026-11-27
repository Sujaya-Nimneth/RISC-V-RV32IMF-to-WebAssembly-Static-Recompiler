//! Shared Virtual Hardware ABI constants, memory map, syscall IDs, and HostEnv trait.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Start of guest linear memory (4 KB null guard page at 0x0000_0000..0x0000_1000).
pub const RAM_START: u32 = 0x0000_0000;
/// Default ELF .text load base address.
pub const TEXT_BASE: u32 = 0x0001_0000;
/// End of 16 MB guest RAM region (exclusive).
pub const RAM_END: u32 = 0x0100_0000;
/// Upper bound of upward bump heap (`sys_sbrk`) leaving 2 MB for downward stack.
pub const HEAP_LIMIT: u32 = 0x00E0_0000;
/// Initial 16-byte-aligned stack pointer (`sp = x2`).
pub const STACK_TOP: u32 = 0x00FF_FFF0;

/// Base address of memory-mapped RGBA8888 framebuffer.
pub const FB_BASE: u32 = 0x0100_0000;
/// Maximum supported framebuffer width in pixels.
pub const FB_MAX_WIDTH: u32 = 640;
/// Maximum supported framebuffer height in pixels.
pub const FB_MAX_HEIGHT: u32 = 480;
/// Maximum framebuffer byte size (`640 * 480 * 4 = 1,228,800`).
pub const FB_MAX_SIZE: u32 = FB_MAX_WIDTH * FB_MAX_HEIGHT * 4;
/// End of framebuffer region (exclusive).
pub const FB_END: u32 = FB_BASE + FB_MAX_SIZE;
/// Default framebuffer width in pixels.
pub const DEFAULT_FB_WIDTH: u32 = 320;
/// Default framebuffer height in pixels.
pub const DEFAULT_FB_HEIGHT: u32 = 200;

/// Base address of 4 KB MMIO control and input register block.
pub const MMIO_BASE: u32 = 0x0120_0000;
pub const MMIO_KEY_LO: u32 = 0x00;
pub const MMIO_KEY_HI: u32 = 0x04;
pub const MMIO_MOUSE_X: u32 = 0x08;
pub const MMIO_MOUSE_Y: u32 = 0x0C;
pub const MMIO_MOUSE_BTN: u32 = 0x10;
pub const MMIO_CLOCK_MS: u32 = 0x14;
pub const MMIO_FB_WIDTH: u32 = 0x18;
pub const MMIO_FB_HEIGHT: u32 = 0x1C;
pub const MMIO_FRAME_COUNT: u32 = 0x20;

/// Single WebAssembly linear memory page count (290 * 64 KiB = 18.56 MiB).
pub const LINEAR_MEM_PAGES: u32 = 290;
/// Total linear memory size in bytes (`0x0122_0000`).
pub const LINEAR_MEM_BYTES: usize = (LINEAR_MEM_PAGES as usize) * 65_536;

/// `sys_write(fd, ptr, len) -> bytes_written | -1`
pub const SYS_WRITE: u32 = 1;
/// `sys_exit(exit_code) -> !`
pub const SYS_EXIT: u32 = 2;
/// `sys_clock_ms() -> u32`
pub const SYS_CLOCK_MS: u32 = 3;
/// `sys_present_frame(width, height) -> 0`
pub const SYS_PRESENT_FRAME: u32 = 4;
/// `sys_poll_input() -> key_mask_lo`
pub const SYS_POLL_INPUT: u32 = 5;
/// `sys_sbrk(increment) -> prev_brk | -1`
pub const SYS_SBRK: u32 = 6;

/// Keyboard/mouse bitmasks and coordinates exposed via MMIO.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputState {
    pub keys_lo: u32,
    pub keys_hi: u32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_btn: u32,
}

/// Action returned by `HostEnv::ecall` to control VM execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction {
    Continue,
    Exit(i32),
}

#[derive(Debug, Error)]
pub enum AbiError {
    #[error("memory access out of bounds at 0x{addr:08x} (len {len})")]
    OutOfBounds { addr: u32, len: usize },
}

/// Host environment callbacks implementing the Virtual Hardware ABI.
pub trait HostEnv {
    /// Write bytes to guest stdout (`fd == 1`) or stderr (`fd == 2`).
    fn write_fd(&mut self, fd: u32, data: &[u8]) -> i32;

    /// Called when the guest executes `sys_exit`.
    fn on_exit(&mut self, code: i32);

    /// Return current monotonic clock in milliseconds.
    fn clock_ms(&mut self) -> u32;

    /// Signal that a frame has been rendered to `FB_BASE`.
    fn present_frame(&mut self, width: u32, height: u32, fb_bytes: &[u8]);

    /// Return the latest input state to populate MMIO.
    fn poll_input(&mut self) -> InputState;
}

/// Deterministic headless host used by tests, CLI, and differential checks.
#[derive(Debug, Clone, Default)]
pub struct HeadlessHost {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub clock_ms_value: u32,
    pub frame_count: u32,
    pub fb_width: u32,
    pub fb_height: u32,
    pub last_frame: Vec<u8>,
    pub input: InputState,
}

impl HeadlessHost {
    pub fn new() -> Self {
        Self {
            fb_width: DEFAULT_FB_WIDTH,
            fb_height: DEFAULT_FB_HEIGHT,
            ..Self::default()
        }
    }

    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

impl HostEnv for HeadlessHost {
    fn write_fd(&mut self, fd: u32, data: &[u8]) -> i32 {
        match fd {
            1 => {
                self.stdout.extend_from_slice(data);
                data.len() as i32
            }
            2 => {
                self.stderr.extend_from_slice(data);
                data.len() as i32
            }
            _ => -1,
        }
    }

    fn on_exit(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    fn clock_ms(&mut self) -> u32 {
        self.clock_ms_value
    }

    fn present_frame(&mut self, width: u32, height: u32, fb_bytes: &[u8]) {
        if width > 0 && width <= FB_MAX_WIDTH {
            self.fb_width = width;
        }
        if height > 0 && height <= FB_MAX_HEIGHT {
            self.fb_height = height;
        }
        self.frame_count = self.frame_count.wrapping_add(1);
        self.last_frame.clear();
        self.last_frame.extend_from_slice(fb_bytes);
    }

    fn poll_input(&mut self) -> InputState {
        self.input
    }
}

/// Write standard MMIO input/clock/fb fields into a linear memory slice.
pub fn write_mmio_input(
    mem: &mut [u8],
    input: InputState,
    clock_ms: u32,
    fb_width: u32,
    fb_height: u32,
    frame_count: u32,
) {
    let base = MMIO_BASE as usize;
    if base + 0x24 > mem.len() {
        return;
    }
    mem[base + MMIO_KEY_LO as usize..base + MMIO_KEY_LO as usize + 4]
        .copy_from_slice(&input.keys_lo.to_le_bytes());
    mem[base + MMIO_KEY_HI as usize..base + MMIO_KEY_HI as usize + 4]
        .copy_from_slice(&input.keys_hi.to_le_bytes());
    mem[base + MMIO_MOUSE_X as usize..base + MMIO_MOUSE_X as usize + 4]
        .copy_from_slice(&input.mouse_x.to_le_bytes());
    mem[base + MMIO_MOUSE_Y as usize..base + MMIO_MOUSE_Y as usize + 4]
        .copy_from_slice(&input.mouse_y.to_le_bytes());
    mem[base + MMIO_MOUSE_BTN as usize..base + MMIO_MOUSE_BTN as usize + 4]
        .copy_from_slice(&input.mouse_btn.to_le_bytes());
    mem[base + MMIO_CLOCK_MS as usize..base + MMIO_CLOCK_MS as usize + 4]
        .copy_from_slice(&clock_ms.to_le_bytes());
    mem[base + MMIO_FB_WIDTH as usize..base + MMIO_FB_WIDTH as usize + 4]
        .copy_from_slice(&fb_width.to_le_bytes());
    mem[base + MMIO_FB_HEIGHT as usize..base + MMIO_FB_HEIGHT as usize + 4]
        .copy_from_slice(&fb_height.to_le_bytes());
    mem[base + MMIO_FRAME_COUNT as usize..base + MMIO_FRAME_COUNT as usize + 4]
        .copy_from_slice(&frame_count.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_map_regions_do_not_overlap() {
        assert!(TEXT_BASE >= 0x1000);
        assert!(HEAP_LIMIT < STACK_TOP);
        assert!(STACK_TOP < RAM_END);
        assert_eq!(RAM_END, FB_BASE);
        assert_eq!(FB_END, 0x0112_C000);
        assert!(FB_END < MMIO_BASE);
        assert!(MMIO_BASE as usize + 0x1000 <= LINEAR_MEM_BYTES);
    }

    #[test]
    fn headless_host_captures_stdio_and_frames() {
        let mut host = HeadlessHost::new();
        assert_eq!(host.write_fd(1, b"hello "), 6);
        assert_eq!(host.write_fd(2, b"warn"), 4);
        assert_eq!(host.write_fd(9, b"bad"), -1);
        assert_eq!(host.stdout_str(), "hello ");
        assert_eq!(host.stderr_str(), "warn");

        host.present_frame(320, 200, &[0xAA, 0xBB, 0xCC, 0xFF]);
        assert_eq!(host.frame_count, 1);
        assert_eq!(host.last_frame, vec![0xAA, 0xBB, 0xCC, 0xFF]);
        host.on_exit(42);
        assert_eq!(host.exit_code, Some(42));
    }
}
