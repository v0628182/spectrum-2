import React from 'react';
import { Crosshair } from 'lucide-react';
import './audio-presets.css';
import { PROFILE_TO_PRESET, ROUTING_PRESETS, type PresetId } from './presets';

interface AudioPresetsProps {
  activeProfile: number | null;
  busyLabel: string | null;
  canInteract: boolean;
  engineActive: boolean;
  onSelect: (presetId: PresetId) => void;
  selectedPreset: PresetId | null;
}

export const AudioPresets: React.FC<AudioPresetsProps> = ({
  activeProfile,
  busyLabel,
  canInteract,
  engineActive,
  onSelect,
  selectedPreset,
}) => {
  const resolvedPreset = engineActive && activeProfile && PROFILE_TO_PRESET[activeProfile]
    ? PROFILE_TO_PRESET[activeProfile]
    : selectedPreset;
  const selectedPresetMeta = ROUTING_PRESETS.find((preset) => preset.id === resolvedPreset);
  const statusLabel = busyLabel
    ?? (engineActive && activeProfile
      ? `${selectedPresetMeta?.name ?? 'Preset'} applied`
      : 'No preset active');

  return (
    <div className={`opt-card ${engineActive ? 'glow-panel' : ''}`}>
      <div className="opt-card-header">
        <Crosshair size={20} />
        <span className="opt-card-title">ROUTING PRESETS</span>
      </div>

      <div
        className="presets-container"
        style={{
          opacity: canInteract || engineActive ? 1 : 0.35,
          pointerEvents: canInteract ? 'auto' : 'none',
          transition: 'opacity var(--dur-normal) var(--ease-snappy)',
          overflowY: 'auto',
          overflowX: 'hidden',
          paddingRight: '16px',
        }}
      >
        {ROUTING_PRESETS.map((preset) => {
          const Icon = preset.icon;
          const isSelected = resolvedPreset === preset.id;

          return (
            <button
              key={preset.id}
              className={`preset-btn ${isSelected ? 'active' : ''}`}
              onClick={() => onSelect(preset.id)}
            >
              <div className="btn-inner">
                <div className="btn-icon">
                  <Icon size={18} />
                </div>
                <div className="btn-text">
                  <span className="btn-name">{preset.name}</span>
                </div>
                <div className="btn-status-light"></div>
              </div>
            </button>
          );
        })}
      </div>

      <div className="eq-status" style={{ marginTop: 'auto' }}>
        <div className={`eq-status-indicator ${engineActive ? '' : 'offline'}`}></div>
        <span>{statusLabel}</span>
      </div>
    </div>
  );
};
