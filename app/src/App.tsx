import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MainHUD } from './pages/MainHUD';
import { AuthHUD } from './pages/AuthHUD';
import { InstallerHUD } from './pages/InstallerHUD';
import { UpdateGate } from './features/update-gate/UpdateGate';
import { useAuth } from './features/auth/useAuth';
import { installConsoleInterceptor } from './features/devlog/DevLogPanel';
import type { RuntimeSnapshot } from './lib/vanysound';
import './index.css';
import './shared/ui.css';
import './pages/installer.css';

// Install ONCE at module load — captures ALL console output from here on
installConsoleInterceptor();

// ── Disable right-click globally for a native app feel ──
if (typeof window !== 'undefined') {
  window.addEventListener('contextmenu', (e) => e.preventDefault());
}

type ReceiptStatus = 'valid' | 'missing' | 'version_mismatch' | 'corrupted';

/**
 * Boot flow — receipt-based onboarding:
 *
 *   1. UpdateGate → connectivity + version + kill switch
 *   2. AuthHUD → mandatory login/signup (no skip)
 *   3. Backend check: refresh_runtime() → receipt_status + installed
 *      a. Receipt valid + installed → MainHUD (0 friction)
 *      b. Receipt missing/mismatch → InstallerHUD (consent + install)
 *      c. Install complete → MainHUD
 */
function App() {
  const auth = useAuth();
  const [runtimeSnapshot, setRuntimeSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [runtimeChecked, setRuntimeChecked] = useState(false);
  const [installCompleted, setInstallCompleted] = useState(false);

  /* ── On mount: check receipt + runtime via Rust backend ── */
  useEffect(() => {
    let cancelled = false;

    async function checkRuntime() {
      try {
        const snapshot = await invoke<RuntimeSnapshot>('refresh_runtime');
        if (cancelled) return;
        setRuntimeSnapshot(snapshot);
      } catch (err) {
        console.warn('[app] Could not verify runtime:', err);
      } finally {
        if (!cancelled) setRuntimeChecked(true);
      }
    }

    checkRuntime();
    return () => { cancelled = true; };
  }, []);

  const receiptStatus = (runtimeSnapshot?.receiptStatus ?? 'missing') as ReceiptStatus;
  const isRuntimeInstalled = runtimeSnapshot?.installed ?? false;
  const needsSetup = receiptStatus !== 'valid' || !isRuntimeInstalled;

  useEffect(() => {
    console.info('[app] state', {
      isAuthenticated: auth.isAuthenticated,
      isLoading: auth.isLoading,
      receiptStatus,
      isRuntimeInstalled,
      needsSetup,
      installCompleted,
      runtimeChecked,
      user: auth.user?.email ?? null,
    });
  }, [auth.isAuthenticated, auth.isLoading, receiptStatus, isRuntimeInstalled, needsSetup, installCompleted, runtimeChecked, auth.user]);

  /* ── Loading state — checking persisted session ── */
  if (auth.isLoading) {
    return (
      <div className="optimizer-container" style={{
        background: 'var(--bg-base)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
      }}>
        <div style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '11px',
          color: 'rgba(255,255,255,0.3)',
          textTransform: 'uppercase',
          letterSpacing: '2px',
        }}>
          RESTORING SESSION...
        </div>
      </div>
    );
  }

  /* ── Auth gate — mandatory, no skip ── */
  if (!auth.isAuthenticated) {
    return (
      <AuthHUD
        onLogin={auth.handleLogin}
      />
    );
  }

  /* ── Installer gate — receipt-based ── */
  if (runtimeChecked && needsSetup && !installCompleted) {
    return (
      <InstallerHUD
        receiptStatus={receiptStatus}
        onComplete={() => setInstallCompleted(true)}
      />
    );
  }

  /* ── Main app ── */
  return <MainHUD onRequestRepair={() => {
    setRuntimeSnapshot(prev => prev ? { ...prev, receiptStatus: 'corrupted' } : null);
    setInstallCompleted(false);
  }} />;
}

/** Root wrapper — UpdateGate runs BEFORE anything else */
function AppWithGate() {
  return (
    <UpdateGate>
      <App />
    </UpdateGate>
  );
}

export default AppWithGate;
