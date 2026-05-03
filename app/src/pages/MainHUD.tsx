import { useEffect, useRef, useState } from 'react';
import { Cpu } from 'lucide-react';
import { CustomTitlebar } from '../widgets/CustomTitlebar';
import { AudioPresets } from '../features/audio-presets/AudioPresets';
import {
  DEFAULT_PRESET,
  PRESET_TO_PROFILE,
  PROFILE_TO_PRESET,
  type PresetId,
} from '../features/audio-presets/presets';
import { SpatialRadar } from '../features/spatial-radar/SpatialRadar';
import { MicAI } from '../features/mic-ai/MicAI';
import { devLog } from '../features/devlog/DevLogPanel';
import { DspEnginePanel } from '../features/dsp-engine/DspEnginePanel';
import {
  vanySoundApi,
  resolveActiveProfile,
  type RadarSnapshot,
  type RuntimeSnapshot,
} from '../lib/vanysound';
import './hud.css';

const POLL_INTERVAL_MS = 4000;
const RADAR_POLL_INTERVAL_MS = 350;
const MUTATION_SETTLE_MS = 350;

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface MainHUDProps {
  onRequestRepair?: () => void;
}

export const MainHUD: React.FC<MainHUDProps> = ({ onRequestRepair }) => {
  const [runtime, setRuntime] = useState<RuntimeSnapshot | null>(null);
  const [radar, setRadar] = useState<RadarSnapshot | null>(null);
  const [selectedPreset, setSelectedPreset] = useState<PresetId | null>(null);
  const [busyLabel, setBusyLabel] = useState<string | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [dspPanelOpen, setDspPanelOpen] = useState(false);
  const interactionLockRef = useRef(false);

  const syncDebugReport = async () => {
    await vanySoundApi.getDebugReport();
  };

  const syncRuntime = async () => {
    devLog.info('Runtime', 'Syncing runtime snapshot...');
    const snapshot = await vanySoundApi.refreshRuntime();
    setRuntime(snapshot);
    setRuntimeError(snapshot.runtimeError);

    const health = snapshot.installHealth ?? 'unknown';
    const profile = snapshot.status?.activeProfile ?? 0;
    const endpoint = snapshot.status?.targetEndpointName ?? 'none';
    devLog.success('Runtime', `Synced OK — health=${health} profile=${profile} endpoint="${endpoint}"`);

    if (snapshot.runtimeError) {
      devLog.error('Runtime', `Runtime error: ${snapshot.runtimeError}`);
    }

    const activeProfile = resolveActiveProfile(snapshot);
    if (activeProfile && PROFILE_TO_PRESET[activeProfile]) {
      setSelectedPreset((current) => {
        const currentProfile = current ? PRESET_TO_PROFILE[current] : undefined;
        return currentProfile === activeProfile ? current : PROFILE_TO_PRESET[activeProfile];
      });
    }

    await syncDebugReport().catch(() => undefined);
    return snapshot;
  };

  /* ── On mount: clear any active profile so audio starts native ── */
  useEffect(() => {
    let cancelled = false;

    const initializeClean = async () => {
      try {
        // Reset audio to native on app open — user must pick a preset
        await vanySoundApi.clearProfile().catch(() => {});
        const snapshot = await vanySoundApi.refreshRuntime();
        if (cancelled) return;
        setRuntime(snapshot);
        setRuntimeError(snapshot.runtimeError);
        // Intentionally NOT syncing selectedPreset — start blank
        void syncDebugReport().catch(() => undefined);
      } catch (error) {
        if (!cancelled) {
          setRuntimeError(getErrorMessage(error));
          void syncDebugReport().catch(() => undefined);
        }
      }
    };

    void initializeClean();
    const interval = window.setInterval(async () => {
      if (cancelled) return;
      try {
        const snapshot = await vanySoundApi.refreshRuntime();
        if (cancelled) return;
        setRuntime(snapshot);
        setRuntimeError(snapshot.runtimeError);
        const activeProfile = resolveActiveProfile(snapshot);
        if (activeProfile && PROFILE_TO_PRESET[activeProfile]) {
          setSelectedPreset((current) => {
            const currentProfile = current ? PRESET_TO_PROFILE[current] : undefined;
            return currentProfile === activeProfile ? current : PROFILE_TO_PRESET[activeProfile];
          });
        }
      } catch (error) {
        if (!cancelled) setRuntimeError(getErrorMessage(error));
      }
    }, POLL_INTERVAL_MS);
    return () => { cancelled = true; window.clearInterval(interval); };
  }, []);

  /* ── On close: clear profile so audio returns to native ── */
  useEffect(() => {
    const clearOnExit = () => {
      // Fire-and-forget: best effort to clear profile before window dies
      void vanySoundApi.clearProfile().catch(() => {});
    };
    window.addEventListener('beforeunload', clearOnExit);
    return () => window.removeEventListener('beforeunload', clearOnExit);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const loadRadar = async () => {
      try {
        const snapshot = await vanySoundApi.getRadarSnapshot();
        if (!cancelled) setRadar(snapshot);
      } catch (error) {
        if (!cancelled) setRuntimeError(getErrorMessage(error));
      }
    };
    void loadRadar();
    const interval = window.setInterval(() => void loadRadar(), RADAR_POLL_INTERVAL_MS);
    return () => { cancelled = true; window.clearInterval(interval); };
  }, []);

  const runRuntimeMutation = async (
    label: string,
    mutation: () => Promise<RuntimeSnapshot>,
  ) => {
    if (interactionLockRef.current) return runtime ?? await syncRuntime();
    interactionLockRef.current = true;
    setBusyLabel(label);
    devLog.info('Mutation', `Starting: ${label}`);
    try {
      const snapshot = await mutation();
      devLog.success('Mutation', `Completed: ${label}`);
      await new Promise<void>((resolve) => window.setTimeout(resolve, MUTATION_SETTLE_MS));
      const settledSnapshot = await syncRuntime().catch(() => snapshot);
      setRuntime(settledSnapshot);
      setRuntimeError(settledSnapshot.runtimeError);
      const activeProfile = resolveActiveProfile(settledSnapshot);
      if (activeProfile && PROFILE_TO_PRESET[activeProfile]) {
        setSelectedPreset((current) => {
          const currentProfile = current ? PRESET_TO_PROFILE[current] : undefined;
          return currentProfile === activeProfile ? current : PROFILE_TO_PRESET[activeProfile];
        });
      }
      return settledSnapshot;
    } catch (error) {
      const errorMsg = getErrorMessage(error);
      devLog.error('Mutation', `FAILED: ${label} — ${errorMsg}`);
      setRuntimeError(errorMsg);
      await syncDebugReport().catch(() => undefined);
      throw error;
    } finally {
      interactionLockRef.current = false;
      setBusyLabel(null);
    }
  };

  const activeProfile = resolveActiveProfile(runtime);
  const engineActive = activeProfile !== null;
  const canInteract = busyLabel === null;

  const handlePresetSelect = async (presetId: PresetId) => {
    if (interactionLockRef.current) return;
    
    // If the clicked preset is already active, toggle the engine OFF
    if (engineActive && activeProfile !== null && PROFILE_TO_PRESET[activeProfile] === presetId) {
      await runRuntimeMutation('Clearing active processing...', () => vanySoundApi.clearProfile());
      setSelectedPreset(null);
      return;
    }

    const previousPreset = selectedPreset;
    setSelectedPreset(presetId);
    const profileId = PRESET_TO_PROFILE[presetId] ?? PRESET_TO_PROFILE[DEFAULT_PRESET]!;
    try {
      await runRuntimeMutation(`Applying ${presetId.toUpperCase()}...`, () => vanySoundApi.switchProfile(profileId));
    } catch {
      setSelectedPreset(previousPreset);
    }
  };



  const runtimeNotInstalled = runtime !== null && !runtime.installed;

  /* ── JSX identical to new app design ── */
  return (
    <div className="optimizer-container">
      <CustomTitlebar
        busyLabel={busyLabel ?? runtimeError}
        onRequestRepair={onRequestRepair}
        runtime={runtime}
      />

      <div className="optimizer-wrapper">
        {runtimeNotInstalled && (
          <div style={{
            background: 'rgba(238, 255, 0, 0.04)',
            border: '1px solid rgba(238, 255, 0, 0.12)',
            borderRadius: 8,
            padding: '10px 16px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            margin: '8px 0 4px',
            gap: 12,
          }}>
            <span style={{
              fontFamily: 'var(--font-mono, monospace)',
              fontSize: '10px',
              color: 'rgba(238, 255, 0, 0.7)',
              textTransform: 'uppercase' as const,
              letterSpacing: '1px',
              fontWeight: 700,
            }}>
              ⚡ Audio drivers not set up yet. Some features won't work.
            </span>
            <button
              onClick={() => onRequestRepair?.()}
              style={{
                background: 'rgba(238, 255, 0, 0.1)',
                border: '1px solid rgba(238, 255, 0, 0.25)',
                borderRadius: 4,
                color: '#eeff00',
                fontFamily: 'var(--font-mono, monospace)',
                fontSize: '9px',
                fontWeight: 800,
                textTransform: 'uppercase' as const,
                letterSpacing: '1px',
                padding: '5px 12px',
                cursor: 'pointer',
                whiteSpace: 'nowrap' as const,
                transition: 'all 0.2s ease',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = 'rgba(238, 255, 0, 0.2)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = 'rgba(238, 255, 0, 0.1)';
              }}
            >
              Set up now
            </button>
          </div>
        )}

        <header className="vanysound-monolith">
          <h1 className={`vany-brand ${engineActive ? 'active' : ''}`}>
            VANY SOUND
          </h1>
          <div className={`vany-status ${busyLabel ? 'visible' : ''}`}>
            {busyLabel || 'SYSTEM READY'}
          </div>
        </header>

        <main className="opt-grid" style={{ marginTop: '8px' }}>
          <MicAI engineActive={engineActive} />
          <AudioPresets
            activeProfile={activeProfile}
            busyLabel={busyLabel ?? runtimeError}
            canInteract={canInteract && !runtimeNotInstalled}
            engineActive={engineActive}
            onSelect={(presetId) => void handlePresetSelect(presetId)}
            selectedPreset={selectedPreset}
          />
          <SpatialRadar
            canInteract={canInteract && !runtimeNotInstalled}
            engineActive={engineActive}
            onModeChange={async (mode) => { await runRuntimeMutation(`Applying ${mode.toUpperCase()} spatial mode...`, () => vanySoundApi.setSpatialMode(mode)); }}
            radar={radar}
            runtime={runtime}
          />
        </main>

        {/* ── DSP Engine Integration Button ── */}
        <div style={{ marginTop: '12px', padding: '0 0 8px' }}>
          <button
            className="dsp-launch-btn"
            onClick={() => setDspPanelOpen(true)}
          >
            <Cpu size={16} />
            DSP Engine Control — Warzone Audio Core
          </button>
        </div>
      </div>

      {/* ── DSP Engine Full Control Panel ── */}
      <DspEnginePanel
        isOpen={dspPanelOpen}
        onClose={() => setDspPanelOpen(false)}
        engineActive={engineActive}
      />

      {/* <div>
        <DevLogPanel defaultOpen={true} />
      </div> */}
    </div>
  );
};

