import React, { useState, useEffect, useCallback } from 'react';
import './radar-sys.css';
import { Target } from 'lucide-react';

import { type RadarSnapshot, type RuntimeSnapshot, type SpatialMode, vanySoundApi } from '../../lib/vanysound';

interface SpatialRadarProps {
  canInteract?: boolean;
  engineActive: boolean;
  onModeChange?: (mode: SpatialMode) => Promise<void>;
  radar?: RadarSnapshot | null;
  runtime?: RuntimeSnapshot | null;
}

/* ── 5 directional channels mapped to polar positions on a circular radar ──
 *  Matches overlay.rs hotspot layout:
 *    far_left  → 225° (back-left)
 *    left      → 280° (front-left)
 *    center    → 0°/360° (front/top)
 *    right     → 80° (front-right)
 *    far_right → 135° (back-right)
 */
const CHANNEL_MAP: { key: keyof Pick<RadarSnapshot, 'farLeft' | 'left' | 'center' | 'right' | 'farRight'>; angle: number; label: string; color: string }[] = [
  { key: 'farLeft',  angle: 225, label: 'BL', color: '#12BFFF' },
  { key: 'left',     angle: 280, label: 'FL', color: '#14E6FF' },
  { key: 'center',   angle: 0,   label: 'FC', color: '#8EFFFF' },
  { key: 'right',    angle: 80,  label: 'FR', color: '#1CB2FF' },
  { key: 'farRight', angle: 135, label: 'BR', color: '#1A8BFF' },
];

/** Convert polar (angle degrees, distance 0-1) to percentage coords inside the radar circle */
function polarToPercent(angleDeg: number, distance: number): { top: number; left: number } {
  const rad = ((angleDeg - 90) * Math.PI) / 180; // -90 so 0° = top
  const x = 50 + Math.cos(rad) * distance * 40; // 40% max from center
  const y = 50 + Math.sin(rad) * distance * 40;
  return { top: y, left: x };
}

/** Soft ceiling matching overlay.rs soft_ceil — keeps quiet sounds intact, compresses loud ones */
function softCeil(x: number): number {
  const SOFT_KNEE = 0.28;
  const SOFT_MAX = 0.55;
  if (x <= SOFT_KNEE) return x;
  const t = (x - SOFT_KNEE) / (1.0 - SOFT_KNEE);
  return SOFT_KNEE + (SOFT_MAX - SOFT_KNEE) * Math.pow(t, 0.45);
}

const SPECTRUM_LABELS = ['32', '63', '125', '250', '500', '1k', '2k', '4k', '8k', '16k'];

function formatHz(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '--';
  if (value >= 1000) return `${(value / 1000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return `${Math.round(value)}`;
}

// ── Feature flag: flip to `true` to re-enable the mini radar overlay ──
const MINI_RADAR_FEATURE_ENABLED = false;

export const SpatialRadar: React.FC<SpatialRadarProps> = ({
  canInteract: _canInteract,
  engineActive,
  onModeChange: _onModeChange,
  radar,
  runtime: _runtime,
}) => {
  const [radarActive, setRadarActive] = useState(false);
  const [miniRadarActive, setMiniRadarActive] = useState(false);
  const [miniRadarPos, setMiniRadarPos] = useState(3); // 0=TL,1=TR,2=BL,3=BR

  const [sweepAngle, setSweepAngle] = useState(0);

  const isLive = radarActive && radar?.captureActive;

  // Radar sweep animation (visual only — 60fps rotation)
  useEffect(() => {
    if (!radarActive) {
      setSweepAngle(0);
      return;
    }
    const timer = setInterval(() => {
      setSweepAngle((prev) => (prev + 2.4) % 360);
    }, 16);
    return () => clearInterval(timer);
  }, [radarActive]);

  const getStatusText = useCallback(() => {
    if (!radarActive) return 'ECHO RADAR OFF';
    if (radar?.lastError) return `ERR: ${radar.lastError}`;
    if (radar?.captureActive) {
      return 'ECHO RADAR ON';
    }
    return 'Initializing capture...';
  }, [radarActive, radar]);

  // Build directional blips from real backend data
  const directionalBlips = CHANNEL_MAP.map((ch) => {
    const raw = radar?.[ch.key] ?? 0;
    const intensity = softCeil(raw);
    // Distance from center: louder = closer to edge (more urgent feel)
    const distance = 0.3 + intensity * 1.4; // 0.3 min, ~1.0 max
    const pos = polarToPercent(ch.angle, Math.min(distance, 1.0));
    // Size scales with intensity
    const size = 6 + intensity * 18; // 6px min, ~15px at full
    return { ...ch, intensity, pos, size };
  });

  // Ambience ring opacity
  const ambience = radar?.ambience ?? 0;
  const spectrum = radar?.spectrum?.length ? radar.spectrum : Array.from({ length: 32 }, () => 0);
  const spectrumPeak = radar?.spectrumPeakHz ?? 0;

  return (
    <div className={`opt-card ${engineActive ? 'glow-panel-acid' : ''}`}>
      <div className="opt-card-header" style={{ marginBottom: 16 }}>
        <Target size={20} />
        <span className="opt-card-title">ECHO RADAR</span>
      </div>

      <div className="radar-screen-wrapper" style={{ position: 'relative', width: '100%', aspectRatio: '1', marginBottom: 16 }}>
        {/* Ambient glow ring */}
        {radarActive && (
          <div
            className="radar-orbit-ring active"
            style={{ opacity: 0.15 + ambience * 0.6 }}
          ></div>
        )}

        <div className="radar-screen" style={{ marginBottom: 0, height: '100%', position: 'absolute' }}>
          <div className="radar-grid-overlay"></div>

          {/* Rotating scanner sweep */}
          {radarActive && (
            <div
              className="radar-scanner active"
              style={{ transform: `rotate(${sweepAngle}deg)` }}
            ></div>
          )}

          {/* Real directional blips from WASAPI spatial analysis */}
          {isLive && directionalBlips.map((blip) => (
            blip.intensity > 0.01 && (
              <div
                key={blip.key}
                style={{
                  position: 'absolute',
                  top: `${blip.pos.top}%`,
                  left: `${blip.pos.left}%`,
                  width: `${blip.size}px`,
                  height: `${blip.size}px`,
                  borderRadius: '50%',
                  background: `radial-gradient(circle, ${blip.color} 0%, transparent 70%)`,
                  boxShadow: `0 0 ${blip.size * 1.5}px ${blip.color}`,
                  opacity: 0.4 + blip.intensity * 1.2,
                  transform: 'translate(-50%, -50%)',
                  transition: 'all 60ms linear',
                  pointerEvents: 'none',
                  zIndex: 10,
                }}
              />
            )
          ))}

          {/* Side glow bars — mimics overlay.rs render_side */}
          {isLive && (
            <>
              {/* Left side glow */}
              <div style={{
                position: 'absolute',
                left: 0,
                top: '10%',
                width: '15%',
                height: '80%',
                background: `linear-gradient(to right, rgba(24,230,255,${((radar?.left ?? 0) + (radar?.farLeft ?? 0)) * 0.4}), transparent)`,
                borderRadius: '0 50% 50% 0',
                pointerEvents: 'none',
                transition: 'all 80ms linear',
              }} />
              {/* Right side glow */}
              <div style={{
                position: 'absolute',
                right: 0,
                top: '10%',
                width: '15%',
                height: '80%',
                background: `linear-gradient(to left, rgba(28,174,255,${((radar?.right ?? 0) + (radar?.farRight ?? 0)) * 0.4}), transparent)`,
                borderRadius: '50% 0 0 50%',
                pointerEvents: 'none',
                transition: 'all 80ms linear',
              }} />
              {/* Top/center glow */}
              <div style={{
                position: 'absolute',
                top: 0,
                left: '10%',
                width: '80%',
                height: '15%',
                background: `linear-gradient(to bottom, rgba(142,255,255,${(radar?.center ?? 0) * 0.5}), transparent)`,
                borderRadius: '0 0 50% 50%',
                pointerEvents: 'none',
                transition: 'all 80ms linear',
              }} />
            </>
          )}

          <div className="radar-center"></div>

          <div className="radar-range-rings">
            <div className="range-ring r-25"></div>
            <div className="range-ring r-50"></div>
            <div className="range-ring r-75"></div>
          </div>
        </div>
      </div>

      <div className="spectrum-live-panel">
        <div className="spectrum-live-header">
          <span>SPECTRUM</span>
          <strong>{formatHz(spectrumPeak)} Hz</strong>
        </div>
        <div className="spectrum-bars" aria-label="Live frequency spectrum">
          {spectrum.map((value, index) => (
            <div className="spectrum-bar-slot" key={`${index}-${spectrum.length}`}>
              <div
                className="spectrum-bar-fill"
                style={{ height: `${Math.max(3, Math.min(100, value * 100))}%` }}
              />
            </div>
          ))}
        </div>
        <div className="spectrum-scale">
          {SPECTRUM_LABELS.map((label) => (
            <span key={label}>{label}</span>
          ))}
        </div>
      </div>

      {/* Radar toggle */}
      <div
        className="radar-toggle-row"
        style={{
          opacity: 1,
          pointerEvents: 'auto',
          transition: 'opacity 0.3s ease',
        }}
      >
        <span className="radar-toggle-label" style={{ color: '#ffffff', fontSize: '12px' }}>RADAR</span>
        <div
          className={`radar-toggle ${radarActive ? 'active' : ''}`}
          onClick={() => {
            const next = !radarActive;
            setRadarActive(next);
            vanySoundApi.setOverlayEnabled(next).catch(() => {});
          }}
        >
          <div className="radar-toggle-thumb"></div>
        </div>
      </div>

      {/* Mini Radar overlay toggle + position — gated by feature flag */}
      {MINI_RADAR_FEATURE_ENABLED && (
      <div className="mini-radar-section">
        <div className="radar-toggle-row">
          <span className="radar-toggle-label" style={{ color: '#ffffff', fontSize: '12px' }}>MINI RADAR</span>
          <div
            className={`radar-toggle ${miniRadarActive ? 'active' : ''}`}
            onClick={() => {
              const next = !miniRadarActive;
              setMiniRadarActive(next);
              vanySoundApi.setMiniRadarEnabled(next).catch(() => {});
            }}
          >
            <div className="radar-toggle-thumb"></div>
          </div>
        </div>

        {miniRadarActive && (
          <div className="mini-radar-pos-grid">
            {[
              { id: 0, label: 'TL' },
              { id: 1, label: 'TR' },
              { id: 2, label: 'BL' },
              { id: 3, label: 'BR' },
            ].map((pos) => (
              <button
                key={pos.id}
                className={`mini-radar-pos-btn ${miniRadarPos === pos.id ? 'active' : ''}`}
                onClick={() => {
                  setMiniRadarPos(pos.id);
                  vanySoundApi.setMiniRadarPosition(pos.id).catch(() => {});
                }}
                title={pos.label}
              >
                <div className="mini-radar-pos-screen">
                  <div className={`mini-radar-pos-dot pos-${pos.label.toLowerCase()}`}></div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>
      )}


      {/* Status with live data readout */}
      <div className="eq-status" style={{ marginTop: 'auto' }}>
        <div className={`eq-status-indicator ${isLive ? 'violet' : radarActive ? 'yellow' : ''}`}></div>
        <span style={{
          color: 'var(--text-primary)',
          fontSize: '11px',
          fontFamily: 'var(--font-mono)',
        }}>
          {getStatusText()}
        </span>
      </div>
    </div>
  );
};
