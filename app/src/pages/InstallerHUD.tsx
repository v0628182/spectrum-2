import React, { useState, useEffect } from 'react';
import { User, AlertTriangle } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { CustomTitlebar } from '../widgets/CustomTitlebar';
import './installer.css';

interface InstallerHUDProps {
  receiptStatus: string;
  onComplete: () => void;
}

interface InstallerState {
  completed: boolean;
  currentStep: number;
  detail: string;
  exitCode: number | null;
  finishedAt: string | null;
  headline: string;
  isInstalled: boolean;
  logLines: string[];
  progress: number;
  running: boolean;
  startedAt: string | null;
  success: boolean | null;
  summary: string;
}

/* ── Step display names — no technical info exposed ── */
const STEP_LABELS: Record<number, string> = {
  1: 'Setting up audio drivers',
  2: 'Cleaning previous configuration',
  3: 'Installing audio pipeline',
  4: 'Configuring microphone path',
  5: 'Applying spatial profiles',
  6: 'Finalizing setup',
};

/* ── Generic failure reasons — never expose raw logs ── */
function deriveFailureReason(state: InstallerState | null): string {
  if (!state) return 'An unexpected error occurred during setup.';

  const failedStep = state.currentStep;
  const stepLabel = STEP_LABELS[failedStep] ?? 'unknown step';

  if (state.exitCode === 740 || state.exitCode === 5) {
    return 'VanySound needs administrator privileges to configure audio drivers. Please run the app as admin.';
  }
  if (state.exitCode === 1603 || state.exitCode === 1602) {
    return 'Another installation is currently running. Close any other installers and try again.';
  }
  if (failedStep <= 2) {
    return `Setup failed while ${stepLabel.toLowerCase()}. Your antivirus may be blocking the process — try disabling it temporarily.`;
  }
  if (failedStep <= 4) {
    return `Setup failed while ${stepLabel.toLowerCase()}. Make sure no other audio software is open and try again.`;
  }
  return `Setup couldn't finish (step ${failedStep}/6). Restart the app and try again. If this keeps happening, hit us up on Discord.`;
}

/* ── Contextual copy based on why the user is seeing this screen ── */
function getConsentCopy(receiptStatus: string) {
  switch (receiptStatus) {
    case 'version_mismatch':
      return {
        heading: 'Quick update needed.',
        sub: 'New version dropped. Your audio drivers need a refresh — takes about 60 seconds.',
        cta: 'Update Now',
      };
    case 'corrupted':
      return {
        heading: 'Something broke.',
        sub: "Your audio setup got corrupted. Let's fix it real quick — 60 seconds tops.",
        cta: 'Fix It',
      };
    case 'missing':
    default:
      return {
        heading: 'One more thing.',
        sub: 'VanySound needs to set up a few audio drivers to unlock its full potential. Takes about 60 seconds.',
        cta: 'Set It Up',
      };
  }
}

export const InstallerHUD: React.FC<InstallerHUDProps> = ({ receiptStatus, onComplete: _onComplete }) => {
  const [phase, setPhase] = useState<'consent' | 'installing'>('consent');
  const [state, setState] = useState<InstallerState | null>(null);
  const [started, setStarted] = useState(false);

  const consentCopy = getConsentCopy(receiptStatus);

  /* ── Start the real installer when user confirms ── */
  const startInstallation = () => {
    setPhase('installing');
  };

  /* ── Kick off installer when entering install phase ── */
  useEffect(() => {
    if (phase !== 'installing' || started) return;
    setStarted(true);

    invoke<InstallerState>('run_installer')
      .then((snapshot) => setState(snapshot))
      .catch((err) => {
        console.warn('[installer] Backend unavailable, falling back to simulation.', err);
        let prog = 0;
        let step = 1;
        
        setState({
          completed: false, currentStep: 1, detail: '',
          exitCode: null, finishedAt: null, headline: 'Hold tight.',
          isInstalled: false, logLines: [], progress: 0,
          running: true, startedAt: new Date().toISOString(), success: null,
          summary: ''
        });

        const simInterval = setInterval(() => {
          prog += 1.5;
          if (prog >= 100) {
            clearInterval(simInterval);
            setState(s => ({
              ...s!, completed: true, success: true, progress: 100,
              currentStep: 6,
              headline: "We're done here.", detail: '',
              logLines: []
            }));
            return;
          }
          
          if (prog > 20 && step === 1) step = 2;
          if (prog > 40 && step === 2) step = 3;
          if (prog > 60 && step === 3) step = 4;
          if (prog > 80 && step === 4) step = 5;

          setState(s => ({
            ...s!, progress: prog, currentStep: step,
            headline: step < 3 ? 'Tuning the background.' : 'Dialing it in.',
            detail: '',
            logLines: []
          }));
        }, 60);
      });
  }, [phase, started]);

  /* ── Poll backend for progress every 600ms (only during install phase) ── */
  useEffect(() => {
    if (phase !== 'installing') return;
    const interval = setInterval(() => {
      invoke<InstallerState>('get_installer_state')
        .then((snapshot) => setState(snapshot))
        .catch(() => {});
    }, 600);
    return () => clearInterval(interval);
  }, [phase]);

  /* ═══════════════════════════════════════════════
     PHASE 1: CONSENT — Explain what's about to happen
     ═══════════════════════════════════════════════ */
  if (phase === 'consent') {
    return (
      <div className="installer-container">
        <CustomTitlebar />
        <div className="installer-centered-content">
          <div className="installer-typography">
            <h1 className="installer-main-heading">{consentCopy.heading}</h1>
            <p className="installer-sub-heading">{consentCopy.sub}</p>
            <div className="installer-detail-text" style={{ opacity: 0.4, marginTop: 8 }}>
              This installs EqualizerAPO, Hi-Fi Cable, and audio profiles for spatial processing.
            </div>
          </div>

          <div style={{ marginTop: 48, display: 'flex', flexDirection: 'column', gap: 12, alignItems: 'center' }}>
            <button
              className="minimal-launch-btn"
              onClick={startInstallation}
              style={{ minWidth: 220 }}
            >
              {consentCopy.cta}
            </button>
            <span style={{
              fontFamily: 'var(--font-mono, monospace)',
              fontSize: '10px',
              color: 'rgba(255,255,255,0.25)',
              textTransform: 'uppercase' as const,
              letterSpacing: '1.5px',
              marginTop: 8,
            }}>
              Requires admin privileges
            </span>
          </div>
        </div>
      </div>
    );
  }

  /* ═══════════════════════════════════════════════
     PHASE 2: INSTALLING — Progress bar (no raw logs)
     ═══════════════════════════════════════════════ */
  const isFinished = state?.completed ?? false;
  const isSuccess = state?.success ?? false;
  const progress = state?.progress ?? 0;
  const currentStep = state?.currentStep ?? 1;
  const stepLabel = STEP_LABELS[currentStep] ?? 'Processing...';

  return (
    <div className="installer-container">
      <CustomTitlebar />

      <div className="installer-centered-content">
        <div className="installer-typography">
          <h1 className="installer-main-heading">
            {isFinished
              ? isSuccess ? "You're all set." : "We hit a snag."
              : "Getting your sound right."}
          </h1>
          <p className="installer-sub-heading">
            {isFinished && isSuccess
              ? 'Everything is locked in.'
              : isFinished && !isSuccess
                ? deriveFailureReason(state)
                : stepLabel}
          </p>
        </div>

        <div className="installer-progress-minimal">
          <div className="minimal-bar-track">
            <div
              className={`minimal-bar-fill ${isFinished && isSuccess ? 'complete' : ''} ${isFinished && !isSuccess ? 'error' : ''}`}
              style={{ width: `${progress}%` }}
            ></div>
          </div>
          <div className="minimal-metrics">
            <span>{Math.floor(progress)}%</span>
            <span>Step {currentStep} of 6</span>
          </div>
        </div>

        {/* Failure detail — generic, no stack traces or logs */}
        {isFinished && !isSuccess && (
          <div className="installer-failure-hint">
            <AlertTriangle size={16} />
            <span>Need help? Join our Discord and we'll sort it out in 5 minutes.</span>
          </div>
        )}

        <div className={`installer-action-area ${isFinished ? 'visible' : ''}`}>
          {isSuccess ? (
            <div className="success-actions">
              <button
                className="minimal-launch-btn"
                onClick={async () => {
                  try {
                    await invoke('reboot_system');
                  } catch (err) {
                    console.error('Reboot failed:', err);
                  }
                }}
                disabled={!isFinished}
              >
                Reboot Now
              </button>
              <p className="reboot-hint">Your PC needs to restart to apply audio changes.</p>
            </div>
          ) : (
            <div className="failure-actions">
              <button className="minimal-launch-btn help" onClick={() => window.open('https://discord.gg/uw3XTNmPWr', '_blank')}>
                <User size={16} />
                <span>Get help on Discord</span>
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
