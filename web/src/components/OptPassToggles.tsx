import React from 'react';
import type { OptConfig } from '../types';

interface Props {
  config: OptConfig;
  onChange: (next: OptConfig) => void;
}

const PASS_LABELS: { key: keyof OptConfig; label: string }[] = [
  { key: 'zero_reg_elim', label: 'x0 elim' },
  { key: 'register_promotion', label: 'reg promote' },
  { key: 'constant_folding', label: 'const fold' },
  { key: 'memarg_folding', label: 'memarg fold' },
  { key: 'branch_fusion', label: 'branch fuse' },
  { key: 'dce', label: 'DCE' },
  { key: 'local_coalescing', label: 'local coalesce' },
  { key: 'count_instructions', label: 'instret count' },
];

export const OptPassToggles: React.FC<Props> = ({ config, onChange }) => {
  return (
    <div className="opt-grid">
      {PASS_LABELS.map(({ key, label }) => (
        <label key={key} className="opt-item">
          <input
            type="checkbox"
            checked={config[key]}
            onChange={(e) => onChange({ ...config, [key]: e.target.checked })}
          />
          <span>{label}</span>
        </label>
      ))}
    </div>
  );
};
