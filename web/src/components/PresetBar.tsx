import React, { useRef, useState } from 'react';
import { PRESETS, type PresetInfo } from '../types';

interface Props {
  activePresetId: string;
  customName: string | null;
  onSelectPreset: (preset: PresetInfo) => void;
  onUploadElf: (name: string, bytes: Uint8Array) => void;
}

export const PresetBar: React.FC<Props> = ({
  activePresetId,
  customName,
  onSelectPreset,
  onUploadElf,
}) => {
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);

  const handleFile = async (file: File) => {
    if (file.size > 4 * 1024 * 1024) {
      alert('ELF file exceeds 4 MB client-side limit.');
      return;
    }
    const buf = new Uint8Array(await file.arrayBuffer());
    onUploadElf(file.name, buf);
  };

  return (
    <div className="preset-bar">
      {PRESETS.map((p) => (
        <button
          key={p.id}
          className={`preset-btn ${activePresetId === p.id && !customName ? 'active' : ''}`}
          onClick={() => onSelectPreset(p)}
          title={p.description}
        >
          <span>{p.name}</span>
          <span className="isa-tag">{p.isa}</span>
        </button>
      ))}

      <div
        className={`dropzone ${dragging ? 'dragging' : ''}`}
        onClick={() => inputRef.current?.click()}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          const f = e.dataTransfer.files[0];
          if (f) void handleFile(f);
        }}
      >
        {customName ? `Custom: ${customName}` : 'Drop .elf (<= 4 MB) or click to upload'}
        <input
          ref={inputRef}
          type="file"
          accept=".elf,application/octet-stream"
          style={{ display: 'none' }}
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void handleFile(f);
          }}
        />
      </div>
    </div>
  );
};
