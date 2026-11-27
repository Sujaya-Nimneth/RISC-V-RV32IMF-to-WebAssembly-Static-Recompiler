export interface OptConfig {
  zero_reg_elim: boolean;
  register_promotion: boolean;
  constant_folding: boolean;
  memarg_folding: boolean;
  branch_fusion: boolean;
  dce: boolean;
  local_coalescing: boolean;
  count_instructions: boolean;
}

export const DEFAULT_OPT_O2: OptConfig = {
  zero_reg_elim: true,
  register_promotion: true,
  constant_folding: true,
  memarg_folding: true,
  branch_fusion: true,
  dce: true,
  local_coalescing: true,
  count_instructions: true,
};

export const OPT_O0: OptConfig = {
  zero_reg_elim: false,
  register_promotion: false,
  constant_folding: false,
  memarg_folding: false,
  branch_fusion: false,
  dce: false,
  local_coalescing: false,
  count_instructions: true,
};

export type ExecMode = 'recomp-o2' | 'recomp-o0' | 'interp';

export interface FnSummary {
  addr: number;
  name: string;
  block_count: number;
  inst_count: number;
  is_indirect_target: boolean;
}

export interface RecompileMeta {
  valid: boolean;
  validation_error: string | null;
  compile_time_ms: number;
  elf_size: number;
  wasm_size: number;
  entry_pc: number;
  function_count: number;
  dispatch_table_size: number;
  functions: FnSummary[];
}

export interface CfgNodeInfo {
  id: number;
  guest_pc: number;
  end_pc: number;
  inst_count: number;
  lines: { pc: number; word: number; text: string }[];
}

export interface CfgEdgeInfo {
  from: number;
  to: number;
  kind: 'edge' | 'backedge';
}

export interface InspectOutput {
  name: string;
  addr: number;
  disasm: string;
  ir_pre_opt: string;
  ir_post_opt: string;
  cfg: {
    nodes: CfgNodeInfo[];
    edges: CfgEdgeInfo[];
  };
  fn_wat: string;
}

export interface PresetInfo {
  id: string;
  name: string;
  path: string;
  isa: 'RV32IM' | 'RV32IMF';
  kind: 'compute' | 'interactive';
  description: string;
}

export const PRESETS: PresetInfo[] = [
  {
    id: 'raycaster',
    name: '3D Raycaster',
    path: '/presets/raycaster.elf',
    isa: 'RV32IMF',
    kind: 'interactive',
    description: '60 FPS software 3D raycaster with textured walls, minimap & WASD/mouse look',
  },
  {
    id: 'life',
    name: 'Game of Life',
    path: '/presets/life.elf',
    isa: 'RV32IM',
    kind: 'interactive',
    description: "Conway's Game of Life in C (Space=Pause, R=Reseed, Mouse=Paint)",
  },
  {
    id: 'mandelbrot',
    name: 'Mandelbrot',
    path: '/presets/mandelbrot.elf',
    isa: 'RV32IMF',
    kind: 'compute',
    description: '320x200 IEEE-754 single-precision floating-point fractal renderer',
  },
  {
    id: 'dhrystone',
    name: 'Dhrystone',
    path: '/presets/dhrystone.elf',
    isa: 'RV32IM',
    kind: 'compute',
    description: 'Integer systems benchmark stressing branches, pointers, structs & libc',
  },
  {
    id: 'matmul',
    name: 'MatMul 48x48',
    path: '/presets/matmul.elf',
    isa: 'RV32IMF',
    kind: 'compute',
    description: 'Dual integer + FP32 48x48 matrix multiplication benchmark',
  },
  {
    id: 'compliance',
    name: 'ISA Compliance',
    path: '/presets/compliance.elf',
    isa: 'RV32IMF',
    kind: 'compute',
    description: 'RV32IM + RV32F edge-case self-check (DIV/0, INT_MIN/-1, NaN, FMA)',
  },
];

export interface RaceResult {
  interpMs: number;
  interpMips: number;
  o0Ms: number;
  o0Mips: number;
  o2Ms: number;
  o2Mips: number;
  speedup: number;
  instret: number;
}
