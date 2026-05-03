/**
 * DSP Engine Control Panel — Warzone Audio Core
 *
 * Mirrors the calibration UI layout from spectrum-antes with all parameter
 * sections in the exact order shown in the reference screenshots.
 * Includes presets loaded from the spectrum config files.
 */
import React, { useCallback, useMemo, useRef, useState } from 'react';
import { X, Cpu, ChevronDown, Save, Trash2 } from 'lucide-react';
import { vanySoundApi } from '../../lib/vanysound';
import './dsp-engine-panel.css';

/* ═══════════════════════════════════════════════════════════
   PARAMETER DEFINITIONS — exact match to calibration UI
   ═══════════════════════════════════════════════════════════ */

interface DspParam {
  key: string;
  label: string;
  min: number;
  max: number;
  step: number;
  default: number;
  unit: string;
  section: string;
}

const DSP_PARAMS: DspParam[] = [
  { key: 'changeIntensity',     label: 'Nivel de cambio',        min: 0,    max: 200,   step: 1,    default: 100,  unit: '',   section: 'macro' },
  { key: 'subtletyAmount',      label: 'Sutileza',               min: 0,    max: 100,   step: 1,    default: 35,   unit: '',   section: 'macro' },
  { key: 'wetMix',              label: 'Mezcla procesada',       min: 0,    max: 100,   step: 1,    default: 100,  unit: '',   section: 'macro' },

  // ── PASOS ──
  { key: 'footstepEnhance',      label: 'Footstep Enhance',       min: 0,    max: 100,   step: 1,    default: 100,  unit: '',   section: 'pasos' },
  { key: 'stepLowBodyBoostDb',   label: 'Paso: cuerpo bajo',      min: 0,    max: 14,    step: 0.5,  default: 10,   unit: '',   section: 'pasos' },
  { key: 'stepLowMidBoostDb',    label: 'Paso: cuerpo medio',     min: 0,    max: 14,    step: 0.5,  default: 9,    unit: '',   section: 'pasos' },
  { key: 'stepBodyBoostDb',      label: 'Paso: presencia 1.55k',  min: 0,    max: 20,    step: 0.5,  default: 14,   unit: '',   section: 'pasos' },
  { key: 'stepClarityBoostDb',   label: 'Paso: claridad 3.5k',    min: 0,    max: 24,    step: 0.5,  default: 20,   unit: '',   section: 'pasos' },
  { key: 'detectionSensitivity', label: 'Sensibilidad detector',  min: 0,    max: 100,   step: 1,    default: 100,  unit: '',   section: 'pasos' },

  { key: 'stepBodyFreqHz',       label: 'Paso cuerpo Hz',         min: 600,  max: 2600,  step: 10,   default: 1550, unit: '',   section: 'pasos_tuning' },
  { key: 'stepBodyQ',            label: 'Paso cuerpo Q',          min: 0.25, max: 5,     step: 0.05, default: 1.35, unit: '',   section: 'pasos_tuning' },
  { key: 'stepClarityFreqHz',    label: 'Paso claridad Hz',       min: 1800, max: 6200,  step: 25,   default: 3500, unit: '',   section: 'pasos_tuning' },
  { key: 'stepClarityQ',         label: 'Paso claridad Q',        min: 0.25, max: 6,     step: 0.05, default: 1.85, unit: '',   section: 'pasos_tuning' },

  // ── DISPAROS / AIRSTRIKES ──
  { key: 'gunshotReduction',     label: 'Reducción disparos',     min: 0,    max: 100,   step: 1,    default: 85,   unit: '',   section: 'disparos' },
  { key: 'explosionReduction',   label: 'Reducción explosiones',  min: 0,    max: 100,   step: 1,    default: 90,   unit: '',   section: 'disparos' },
  { key: 'weaponMidCutDb',       label: 'Corte arma 1.6k',        min: -48,  max: 0,     step: 1,    default: -22,  unit: '',   section: 'disparos' },
  { key: 'weaponAirCutDb',       label: 'Corte agudos arma',      min: -48,  max: 0,     step: 1,    default: -20,  unit: '',   section: 'disparos' },
  { key: 'sustainedHoldMs',      label: 'Hold ruido largo ms',    min: 100,  max: 1600,  step: 10,   default: 650,  unit: '',   section: 'disparos' },
  { key: 'masterDuckDb',         label: 'Duck maestro arma',      min: -30,  max: 0,     step: 0.5,  default: 0,    unit: '',   section: 'disparos' },
  { key: 'impactDuckDb',         label: 'Duck impactos',          min: -40,  max: 0,     step: 0.5,  default: -16,  unit: '',   section: 'disparos' },

  { key: 'lowShelfFreqHz',       label: 'Bajos shelf Hz',         min: 80,   max: 500,   step: 5,    default: 250,  unit: '',   section: 'band_target' },
  { key: 'lowMidFreqHz',         label: 'Low-mid Hz',             min: 250,  max: 1200,  step: 10,   default: 650,  unit: '',   section: 'band_target' },
  { key: 'lowMidQ',              label: 'Low-mid Q',              min: 0.25, max: 3,     step: 0.05, default: 0.9,  unit: '',   section: 'band_target' },
  { key: 'weaponMidFreqHz',      label: 'Arma medios Hz',         min: 700,  max: 3600,  step: 25,   default: 1600, unit: '',   section: 'band_target' },
  { key: 'weaponMidQ',           label: 'Arma medios Q',          min: 0.25, max: 4,     step: 0.05, default: 0.85, unit: '',   section: 'band_target' },
  { key: 'weaponAirFreqHz',      label: 'Arma agudos Hz',         min: 3000, max: 12000, step: 50,   default: 6500, unit: '',   section: 'band_target' },
  { key: 'weaponAirQ',           label: 'Arma agudos Q',          min: 0.25, max: 5,     step: 0.05, default: 1,    unit: '',   section: 'band_target' },

  // ── STFT GUNSHOT KILLER ──
  { key: 'spectralFloorDb',      label: 'Mascara baja dB',        min: -48,  max: -18,   step: 1,    default: -36,  unit: '',   section: 'stft' },
  { key: 'stftCutoffHz',         label: 'Corte mascara baja Hz',  min: 500,  max: 8000,  step: 100,  default: 2500, unit: '',   section: 'stft' },
  { key: 'stftPreserveDb',       label: 'Preservar pasos dB',     min: -12,  max: 12,    step: 0.5,  default: 0,    unit: '',   section: 'stft' },

  // ── TRANSIENT / LOOKAHEAD ──
  { key: 'transientKill',        label: 'Transient kill',         min: 0,    max: 100,   step: 1,    default: 70,   unit: '',   section: 'transient' },
  { key: 'lookaheadMs',          label: 'Lookahead ms',           min: 0,    max: 2,     step: 0.01, default: 0,    unit: '',   section: 'transient' },

  { key: 'protectionAttackMs',   label: 'Corte attack ms',        min: 0.5,  max: 90,    step: 0.5,  default: 5,    unit: '',   section: 'timing' },
  { key: 'protectionReleaseMs',  label: 'Corte release ms',       min: 25,   max: 900,   step: 5,    default: 170,  unit: '',   section: 'timing' },
  { key: 'boostAttackMs',        label: 'Boost attack ms',        min: 0.5,  max: 90,    step: 0.5,  default: 8,    unit: '',   section: 'timing' },
  { key: 'boostReleaseMs',       label: 'Boost release ms',       min: 20,   max: 900,   step: 5,    default: 160,  unit: '',   section: 'timing' },
  { key: 'limiterReleaseMs',     label: 'Limiter release ms',     min: 5,    max: 250,   step: 1,    default: 50,   unit: '',   section: 'timing' },
  { key: 'stereoWidth',          label: 'Stereo width',           min: 50,   max: 160,   step: 1,    default: 100,  unit: '',   section: 'timing' },

  // ── SALIDA ──
  { key: 'actionDetail',         label: 'Action Detail',          min: 0,    max: 100,   step: 1,    default: 26,   unit: '',   section: 'salida' },
  { key: 'outputCeilingDb',      label: 'Techo salida dB',        min: -12,  max: -0.5,  step: 0.1,  default: -0.5, unit: '',   section: 'salida' },

  // ── EQ / BALANCE FINAL ──
  { key: 'outputTrimDb',         label: 'Output trim dB',         min: -20,  max: 6,     step: 0.5,  default: -7.5, unit: '',   section: 'eq_balance' },
  { key: 'residualReductionDb',  label: 'Reduccion residual dB',  min: -24,  max: 0,     step: 0.5,  default: 0,    unit: '',   section: 'eq_balance' },
  { key: 'footstepGuardAmount',  label: 'Guard pasos residual',   min: 0,    max: 100,   step: 1,    default: 85,   unit: '',   section: 'eq_balance' },
  { key: 'balanceLowDb',         label: 'Balance bajos dB',       min: -12,  max: 12,    step: 0.5,  default: 0,    unit: '',   section: 'eq_balance' },
  { key: 'balanceMidDb',         label: 'Balance medios dB',      min: -12,  max: 12,    step: 0.5,  default: 0,    unit: '',   section: 'eq_balance' },
  { key: 'balanceHighDb',        label: 'Balance agudos dB',      min: -12,  max: 12,    step: 0.5,  default: 0,    unit: '',   section: 'eq_balance' },

  // ── LEVELING ──
  { key: 'footstepLevelerAmount',   label: 'Footstep Volume',     min: 0,    max: 100,   step: 1,    default: 100,  unit: '',   section: 'leveling' },
  { key: 'footstepTargetRmsDb',     label: 'Loudness objetivo',   min: -36,  max: -14,   step: 0.5,  default: -14,  unit: '',   section: 'leveling' },
  { key: 'footstepMaxLiftDb',       label: 'Max Lift dB',         min: 0,    max: 18,    step: 0.5,  default: 12,   unit: '',   section: 'leveling' },
  { key: 'footstepLevelerSpeedMs',  label: 'Velocidad ms',        min: 1,    max: 120,   step: 1,    default: 5,    unit: '',   section: 'leveling' },

  // ── ESTABILIDAD ──
  { key: 'stabilityAmount',      label: 'Estabilidad general',    min: 0,    max: 100,   step: 1,    default: 100,  unit: '',   section: 'estabilidad' },
  { key: 'spectralFloorStab',    label: 'Spectral floor dB',      min: -48,  max: -18,   step: 1,    default: -34,  unit: '',   section: 'estabilidad' },
  { key: 'stableReleaseMs',      label: 'Release estable ms',     min: 80,   max: 500,   step: 10,   default: 260,  unit: '',   section: 'estabilidad' },
  { key: 'protectionPasos',      label: 'Proteccion pasos',       min: 0,    max: 100,   step: 1,    default: 85,   unit: '',   section: 'estabilidad' },
  { key: 'maxCutStepDb',         label: 'Max cambio corte dB',    min: 3,    max: 24,    step: 0.5,  default: 8,    unit: '',   section: 'estabilidad' },
];

const SECTION_META: { id: string; title: string; side: 'left' | 'right' }[] = [
  { id: 'macro',        title: 'CAMBIO / SUTILEZA',      side: 'left' },
  { id: 'pasos',        title: 'PASOS',                  side: 'left' },
  { id: 'pasos_tuning', title: 'PASOS: FRECUENCIA / Q',  side: 'left' },
  { id: 'disparos',     title: 'DISPAROS / AIRSTRIKES',  side: 'left' },
  { id: 'band_target',  title: 'ARMAS / BANDA EXACTA',   side: 'left' },
  { id: 'stft',         title: 'STFT GUNSHOT KILLER',    side: 'right' },
  { id: 'transient',    title: 'TRANSIENT / LOOKAHEAD',  side: 'right' },
  { id: 'timing',       title: 'TIMING / FEEL',          side: 'right' },
  { id: 'salida',       title: 'SALIDA',                 side: 'right' },
  { id: 'eq_balance',   title: 'EQ / BALANCE FINAL',     side: 'right' },
  { id: 'leveling',     title: 'LEVELING',               side: 'right' },
  { id: 'estabilidad',  title: 'ESTABILIDAD',            side: 'right' },
];

/* ═══════════════════════════════════════════════════════════
   PRESETS — from spectrum-antes/config/*.ini
   ═══════════════════════════════════════════════════════════ */

interface Preset {
  id: string;
  label: string;
  values: Record<string, number>;
  flags?: Record<string, boolean>;
  custom?: boolean;
}

const CUSTOM_PRESETS_STORAGE_KEY = 'vanysound.dsp.customPresets.v1';

function loadCustomPresets(): Preset[] {
  if (typeof window === 'undefined') return [];

  try {
    const raw = window.localStorage.getItem(CUSTOM_PRESETS_STORAGE_KEY);
    if (!raw) return [];

    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];

    return parsed
      .filter((item): item is Preset => {
        if (!item || typeof item !== 'object') return false;
        const candidate = item as Partial<Preset>;
        return (
          typeof candidate.id === 'string'
          && typeof candidate.label === 'string'
          && !!candidate.values
          && typeof candidate.values === 'object'
        );
      })
      .map((preset) => ({ ...preset, custom: true }));
  } catch (error) {
    console.warn('[DSP] custom preset load failed:', error);
    return [];
  }
}

function persistCustomPresets(presets: Preset[]) {
  if (typeof window === 'undefined') return;
  window.localStorage.setItem(
    CUSTOM_PRESETS_STORAGE_KEY,
    JSON.stringify(presets.map(({ id, label, values, flags }) => ({ id, label, values, flags }))),
  );
}

function makeCustomPresetId(name: string) {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);

  return `custom-${slug || 'preset'}-${Date.now().toString(36)}`;
}

const PRESETS: Preset[] = [
  {
    id: 'mejoropc',
    label: 'Mejor OPC',
    values: {
      footstepEnhance: 100, actionDetail: 26, gunshotReduction: 85, explosionReduction: 90,
      detectionSensitivity: 100, outputCeilingDb: -0.5, stepBodyBoostDb: 14, stepClarityBoostDb: 20,
      stepLowBodyBoostDb: 10, stepLowMidBoostDb: 9, weaponMidCutDb: -22, weaponAirCutDb: -20,
      sustainedHoldMs: 650, masterDuckDb: 0, impactDuckDb: -16, footstepLevelerAmount: 100,
      footstepTargetRmsDb: -14, footstepMaxLiftDb: 12, footstepLevelerSpeedMs: 10,
      stabilityAmount: 100, spectralFloorDb: -36, stftCutoffHz: 2500, stftPreserveDb: 0,
      transientKill: 70, spectralFloorStab: -34, stableReleaseMs: 260, footstepGuardAmount: 85,
      protectionPasos: 85, maxCutStepDb: 8, outputTrimDb: -7.5, residualReductionDb: 0,
      balanceLowDb: 0, balanceMidDb: 0, balanceHighDb: 0,
    },
    flags: { protectionExtreme: true, spectralMaskEnabled: true, debugLogging: true },
  },
  {
    id: 'warzone_reference',
    label: '🎯 Warzone Reference v1',
    values: {
      footstepEnhance: 100, actionDetail: 26, gunshotReduction: 85, explosionReduction: 90,
      detectionSensitivity: 100, outputCeilingDb: -12, stepBodyBoostDb: 14, stepClarityBoostDb: 20,
      stepLowBodyBoostDb: 10, stepLowMidBoostDb: 9, weaponMidCutDb: -22, weaponAirCutDb: -20,
      sustainedHoldMs: 650, masterDuckDb: 0, impactDuckDb: -16, footstepLevelerAmount: 100,
      footstepTargetRmsDb: -14, footstepMaxLiftDb: 12, footstepLevelerSpeedMs: 10,
      stabilityAmount: 70, spectralFloorDb: -34, stableReleaseMs: 260, footstepGuardAmount: 85,
      maxCutStepDb: 8, outputTrimDb: -7.5,
    },
    flags: { protectionExtreme: true },
  },
  {
    id: 'footstep_focus',
    label: '👟 Footstep Focus',
    values: {
      footstepEnhance: 100, actionDetail: 70, gunshotReduction: 85, explosionReduction: 90,
      detectionSensitivity: 65, outputCeilingDb: -1.0, stepBodyBoostDb: 14, stepClarityBoostDb: 20,
      stepLowBodyBoostDb: 10, stepLowMidBoostDb: 9, weaponMidCutDb: -22, weaponAirCutDb: -20,
      sustainedHoldMs: 650, masterDuckDb: -6, impactDuckDb: -16, footstepLevelerAmount: 65,
      footstepTargetRmsDb: -22, footstepMaxLiftDb: 12, footstepLevelerSpeedMs: 60,
    },
    flags: { protectionExtreme: true },
  },
  {
    id: 'competitive',
    label: '🏆 Competitive Default',
    values: {
      footstepEnhance: 100, actionDetail: 55, gunshotReduction: 100, explosionReduction: 100,
      detectionSensitivity: 55, outputCeilingDb: -6.0, stepBodyBoostDb: 11, stepClarityBoostDb: 18,
      stepLowBodyBoostDb: 8, stepLowMidBoostDb: 7, weaponMidCutDb: -30, weaponAirCutDb: -28,
      sustainedHoldMs: 900, masterDuckDb: -10, impactDuckDb: -24, footstepLevelerAmount: 35,
      footstepTargetRmsDb: -24, footstepMaxLiftDb: 8, footstepLevelerSpeedMs: 80,
    },
    flags: { protectionExtreme: true },
  },
  {
    id: 'extreme_protection',
    label: '🛡️ Extreme Protection',
    values: {
      footstepEnhance: 90, actionDetail: 40, gunshotReduction: 100, explosionReduction: 100,
      detectionSensitivity: 50, outputCeilingDb: -1.5, stepBodyBoostDb: 8, stepClarityBoostDb: 14,
      stepLowBodyBoostDb: 4, stepLowMidBoostDb: 4, weaponMidCutDb: -36, weaponAirCutDb: -36,
      sustainedHoldMs: 1200, masterDuckDb: -16, impactDuckDb: -32, footstepLevelerAmount: 20,
      footstepTargetRmsDb: -25, footstepMaxLiftDb: 6, footstepLevelerSpeedMs: 100,
    },
    flags: { protectionExtreme: true },
  },
  {
    id: 'opcion_2_stable',
    label: '⚡ Opción 2 Stable',
    values: {
      footstepEnhance: 100, actionDetail: 100, gunshotReduction: 91, explosionReduction: 100,
      detectionSensitivity: 100, outputCeilingDb: -11.5, stepBodyBoostDb: 20, stepClarityBoostDb: 24,
      stepLowBodyBoostDb: 13.5, stepLowMidBoostDb: 14, weaponMidCutDb: -48, weaponAirCutDb: -28,
      sustainedHoldMs: 1600, masterDuckDb: -10, impactDuckDb: -24, footstepLevelerAmount: 45,
      footstepTargetRmsDb: -23, footstepMaxLiftDb: 10, footstepLevelerSpeedMs: 70,
      stabilityAmount: 72, spectralFloorDb: -34, stableReleaseMs: 280, footstepGuardAmount: 88,
      maxCutStepDb: 8,
    },
    flags: { protectionExtreme: true },
  },
  {
    id: 'neww',
    label: '🔥 Max Kill (Neww)',
    values: {
      footstepEnhance: 100, actionDetail: 45, gunshotReduction: 100, explosionReduction: 100,
      detectionSensitivity: 100, outputCeilingDb: -1, stepBodyBoostDb: 20, stepClarityBoostDb: 24,
      stepLowBodyBoostDb: 14, stepLowMidBoostDb: 14, weaponMidCutDb: -48, weaponAirCutDb: -48,
      sustainedHoldMs: 1000, masterDuckDb: -30, impactDuckDb: -40, footstepLevelerAmount: 100,
      footstepTargetRmsDb: -14, footstepMaxLiftDb: 18, footstepLevelerSpeedMs: 20,
      stabilityAmount: 60, spectralFloorDb: -48, stableReleaseMs: 200, footstepGuardAmount: 95,
      maxCutStepDb: 48,
    },
    flags: { protectionExtreme: true },
  },
];

/* ═══════════════════════════════════════════════════════════
   COMPONENT
   ═══════════════════════════════════════════════════════════ */

interface DspEnginePanelProps {
  isOpen: boolean;
  onClose: () => void;
  engineActive: boolean;
}

export const DspEnginePanel: React.FC<DspEnginePanelProps> = ({
  isOpen,
  onClose,
}) => {
  const [params, setParams] = useState<Record<string, number>>(() => {
    const initial: Record<string, number> = {};
    DSP_PARAMS.forEach((p) => { initial[p.key] = p.default; });
    return initial;
  });
  const [protectionExtreme, setProtectionExtreme] = useState(true);
  const [spectralMask, setSpectralMask] = useState(true);
  const [dspEnabled, setDspEnabled] = useState(false);
  const [applying, setApplying] = useState(false);
  const [engineConnected, setEngineConnected] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);
  const [activePreset, setActivePreset] = useState<string | null>(null);
  const [presetOpen, setPresetOpen] = useState(false);
  const [customPresets, setCustomPresets] = useState<Preset[]>(loadCustomPresets);
  const [presetName, setPresetName] = useState('');
  const [saveStatus, setSaveStatus] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const allPresets = useMemo(() => [...PRESETS, ...customPresets], [customPresets]);

  const flushToBackend = useCallback((allParams: Record<string, number>, extreme: boolean, mask: boolean) => {
    const payload: Record<string, number> = { ...allParams };
    payload.protectionExtreme = extreme ? 1 : 0;
    payload.spectralMaskEnabled = mask ? 1 : 0;
    setApplying(true);
    setLastError(null);
    vanySoundApi.applyDspConfig(payload)
      .then(() => setEngineConnected(true))
      .catch((err) => {
        console.error('[DSP] apply failed:', err);
        setLastError(String(err));
        setEngineConnected(false);
      })
      .finally(() => setApplying(false));
  }, []);

  // Bypass: write empty config to disable all processing
  const flushBypass = useCallback(() => {
    setApplying(true);
    setLastError(null);
    // Send an empty/zeroed params map — backend will write Preamp: 0 dB only
    vanySoundApi.applyDspConfig({ _bypass: 1 })
      .then(() => setEngineConnected(true))
      .catch((err) => {
        console.error('[DSP] bypass failed:', err);
        setLastError(String(err));
      })
      .finally(() => setApplying(false));
  }, []);

  const saveCustomPreset = useCallback(() => {
    const name = presetName.trim();
    if (!name) {
      setSaveStatus('Pon un nombre');
      return;
    }

    const preset: Preset = {
      id: makeCustomPresetId(name),
      label: name,
      values: { ...params },
      flags: {
        protectionExtreme,
        spectralMaskEnabled: spectralMask,
      },
      custom: true,
    };

    setCustomPresets((prev) => {
      const next = [preset, ...prev.filter((item) => item.label.toLowerCase() !== name.toLowerCase())];
      persistCustomPresets(next);
      return next;
    });
    setActivePreset(preset.id);
    setPresetName('');
    setSaveStatus('Preset guardado');
  }, [params, presetName, protectionExtreme, spectralMask]);

  const deleteCustomPreset = useCallback((presetId: string) => {
    setCustomPresets((prev) => {
      const next = prev.filter((preset) => preset.id !== presetId);
      persistCustomPresets(next);
      return next;
    });
    setActivePreset((current) => (current === presetId ? null : current));
    setSaveStatus('Preset eliminado');
  }, []);

  const handlePowerToggle = useCallback(() => {
    const next = !dspEnabled;
    setDspEnabled(next);
    if (next) {
      // Turning ON: apply current params
      flushToBackend(params, protectionExtreme, spectralMask);
    } else {
      // Turning OFF: bypass all processing
      flushBypass();
    }
  }, [dspEnabled, params, protectionExtreme, spectralMask, flushToBackend, flushBypass]);

  const handleParamChange = useCallback((key: string, value: number) => {
    setParams((prev) => {
      const next = { ...prev, [key]: value };
      if (dspEnabled) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        debounceRef.current = setTimeout(() => {
          flushToBackend(next, protectionExtreme, spectralMask);
        }, 120);
      }
      return next;
    });
    setActivePreset(null);
    setSaveStatus(null);
  }, [flushToBackend, protectionExtreme, spectralMask, dspEnabled]);

  const handleReset = useCallback(() => {
    const defaults: Record<string, number> = {};
    DSP_PARAMS.forEach((p) => { defaults[p.key] = p.default; });
    setParams(defaults);
    setProtectionExtreme(true);
    setActivePreset(null);
    setSaveStatus(null);
    flushToBackend(defaults, true, true);
  }, [flushToBackend]);

  const handleLoadPreset = useCallback((preset: Preset) => {
    const next: Record<string, number> = {};
    DSP_PARAMS.forEach((p) => {
      next[p.key] = preset.values[p.key] ?? p.default;
    });
    setParams(next);
    if (preset.flags?.protectionExtreme !== undefined) {
      setProtectionExtreme(preset.flags.protectionExtreme);
    }
    if (preset.flags?.spectralMaskEnabled !== undefined) {
      setSpectralMask(preset.flags.spectralMaskEnabled);
    }
    setActivePreset(preset.id);
    setSaveStatus(null);
    setPresetOpen(false);
    if (!dspEnabled) setDspEnabled(true);
    flushToBackend(
      next,
      preset.flags?.protectionExtreme ?? protectionExtreme,
      preset.flags?.spectralMaskEnabled ?? spectralMask,
    );
  }, [flushToBackend, protectionExtreme, spectralMask, dspEnabled]);

  if (!isOpen) return null;

  const leftSections = SECTION_META.filter((s) => s.side === 'left');
  const rightSections = SECTION_META.filter((s) => s.side === 'right');

  const renderSection = (sectionId: string, title: string) => {
    const sectionParams = DSP_PARAMS.filter((p) => p.section === sectionId);
    return (
      <div className="dsp-section" key={sectionId}>
        <div className="dsp-section-title">{title}</div>

        {/* STFT toggle */}
        {sectionId === 'stft' && (
          <div className="dsp-toggle-row" style={{ marginBottom: 8 }}>
            <label className="dsp-checkbox-label">
              <input
                type="checkbox"
                checked={spectralMask}
                onChange={() => {
                  const next = !spectralMask;
                  setSpectralMask(next);
                  if (dspEnabled) flushToBackend(params, protectionExtreme, next);
                }}
              />
              <span>Activar mascara espectral</span>
            </label>
          </div>
        )}

        {sectionParams.map((param) => (
          <div className="dsp-param-row" key={param.key}>
            <span className="dsp-param-label">{param.label}</span>
            <input
              type="range"
              className="dsp-param-slider"
              min={param.min}
              max={param.max}
              step={param.step}
              value={params[param.key] ?? param.default}
              onChange={(e) => handleParamChange(param.key, parseFloat(e.target.value))}
            />
            <span className="dsp-param-value">
              {(params[param.key] ?? param.default).toFixed(param.step < 1 ? (param.step < 0.1 ? 2 : 1) : 0)}
            </span>
          </div>
        ))}
      </div>
    );
  };

  return (
    <div className="dsp-overlay" onClick={onClose}>
      <div className="dsp-panel dsp-panel-wide" onClick={(e) => e.stopPropagation()}>

        {/* ── Header ── */}
        <div className="dsp-header">
          <div className="dsp-header-left">
            <div className="dsp-header-icon"><Cpu size={18} /></div>
            <div className="dsp-header-text">
              <h2>DSP Engine Control</h2>
              <span>Warzone Audio Core — RealtimeEngine</span>
            </div>
          </div>
          <div className="dsp-header-right">
            {/* ON/OFF Power Toggle */}
            <div
              className={`dsp-power-toggle ${dspEnabled ? 'active' : ''}`}
              onClick={handlePowerToggle}
            >
              <div className="dsp-power-dot" />
              <span className="dsp-power-label">{dspEnabled ? 'ON' : 'OFF'}</span>
            </div>
            {/* Preset selector */}
            <div className="dsp-preset-dropdown">
              <button
                className="dsp-preset-btn"
                onClick={() => setPresetOpen(!presetOpen)}
              >
                {activePreset
                  ? allPresets.find((p) => p.id === activePreset)?.label ?? 'Custom'
                  : 'Seleccionar preset'}
                <ChevronDown size={14} />
              </button>
              {presetOpen && (
                <div className="dsp-preset-list">
                  <div className="dsp-preset-group-label">Factory</div>
                  {PRESETS.map((preset) => (
                    <button
                      key={preset.id}
                      className={`dsp-preset-item ${activePreset === preset.id ? 'active' : ''}`}
                      onClick={() => handleLoadPreset(preset)}
                    >
                      {preset.label}
                    </button>
                  ))}
                  {customPresets.length > 0 && (
                    <>
                      <div className="dsp-preset-group-label">Guardados</div>
                      {customPresets.map((preset) => (
                        <div
                          key={preset.id}
                          className={`dsp-custom-preset-row ${activePreset === preset.id ? 'active' : ''}`}
                        >
                          <button
                            className="dsp-custom-preset-load"
                            onClick={() => handleLoadPreset(preset)}
                          >
                            {preset.label}
                          </button>
                          <button
                            className="dsp-custom-preset-delete"
                            onClick={(event) => {
                              event.stopPropagation();
                              deleteCustomPreset(preset.id);
                            }}
                            aria-label={`Delete ${preset.label}`}
                            title="Delete preset"
                          >
                            <Trash2 size={13} />
                          </button>
                        </div>
                      ))}
                    </>
                  )}
                </div>
              )}
            </div>
            <button className="dsp-close-btn" onClick={onClose} aria-label="Close">
              <X size={18} />
            </button>
          </div>
        </div>

        {/* ── Body: two-column layout ── */}
        <div className={`dsp-body dsp-body-columns ${!dspEnabled ? 'dsp-body-disabled' : ''}`}>
          <div className="dsp-column">
            {leftSections.map((s) => renderSection(s.id, s.title))}
          </div>
          <div className="dsp-column">
            {rightSections.map((s) => renderSection(s.id, s.title))}

            {/* Protection extreme toggle at end of right column */}
            <div className="dsp-section">
              <div className="dsp-section-title">FLAGS</div>
              <div className="dsp-toggle-row">
                <span className="dsp-toggle-label">Protection Extreme</span>
                <div
                  className={`dsp-toggle-switch ${protectionExtreme ? 'on' : ''}`}
                  onClick={() => {
                    const next = !protectionExtreme;
                    setProtectionExtreme(next);
                    flushToBackend(params, next, spectralMask);
                  }}
                />
              </div>
            </div>
          </div>
        </div>

        {/* ── Footer ── */}
        <div className="dsp-footer">
          <div className="dsp-footer-left">
            <div className={`dsp-footer-dot ${engineConnected ? '' : 'offline'}`} />
            <span>
              {applying
                ? 'APLICANDO...'
                : lastError
                  ? 'ERROR — ver consola'
                  : !dspEnabled
                    ? 'ENGINE OFF — BYPASS'
                    : engineConnected
                      ? 'ENGINE ON → CABLE ONLY'
                      : 'MUEVE UN SLIDER PARA CONECTAR'}
            </span>
          </div>
          <div className="dsp-footer-actions">
            <div className="dsp-save-preset">
              <input
                className="dsp-save-input"
                value={presetName}
                onChange={(event) => {
                  setPresetName(event.target.value);
                  setSaveStatus(null);
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') saveCustomPreset();
                }}
                placeholder="Nombre del preset"
                maxLength={40}
              />
              <button className="dsp-save-btn" onClick={saveCustomPreset}>
                <Save size={13} />
                Save as preset
              </button>
              {saveStatus && <span className="dsp-save-status">{saveStatus}</span>}
            </div>
            <button className="dsp-reset-btn" onClick={handleReset}>
              Reset Defaults
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
