import { useEffect, useState, useRef, useCallback } from 'react';
import { check } from '@tauri-apps/plugin-updater';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';

const API_VERSION_URL = 'https://api.vanysound.com/api/version';
const CONNECTIVITY_CHECK_INTERVAL = 15_000; // 15s retry when offline

interface VersionInfo {
  latest: string;
  min_supported: string;
  mandatory: boolean;
  download_url: string;
  release_notes?: string;
  kill_switch?: boolean;
  kill_message?: string;
}

type GateState = 'checking' | 'offline' | 'updating' | 'reboot_required' | 'killed' | 'ok';

/** Semantic version compare with input validation. Returns -1, 0, or 1 */
function compareSemver(a: string, b: string): number {
  if (!a || !b || typeof a !== 'string' || typeof b !== 'string') return 0;
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const va = Number.isFinite(pa[i]) ? pa[i] : 0;
    const vb = Number.isFinite(pb[i]) ? pb[i] : 0;
    if (va < vb) return -1;
    if (va > vb) return 1;
  }
  return 0;
}

/**
 * UpdateGate — Online-only gate + auto-update + kill switch.
 *
 * 1. If no internet → BLOCK with "No Connection" screen
 * 2. If kill switch → BLOCK with "Service Suspended"
 * 3. If outdated → AUTO-UPDATE (download + install + relaunch)
 * 4. If online + current → allow through
 */
export function UpdateGate({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<GateState>('checking');
  const [versionInfo, setVersionInfo] = useState<VersionInfo | null>(null);
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [downloadTotal, setDownloadTotal] = useState(0);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const retryTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const updateStartedRef = useRef(false);

  // Dev mode: skip gate (CORS blocks external API from localhost)
  const isDev = import.meta.env.DEV;
  if (isDev) return <>{children}</>;

  /** Attempt Tauri auto-update: download, install, relaunch */
  const performAutoUpdate = useCallback(async (vInfo: VersionInfo) => {
    if (updateStartedRef.current) return;
    updateStartedRef.current = true;

    try {
      const update = await check();

      if (!update) {
        // Tauri plugin says no update but our API disagrees —
        // don't silently pass through, show manual download fallback
        setUpdateError(
          `Version mismatch: server reports v${vInfo.latest} but updater found nothing. Download manually.`
        );
        return;
      }

      let downloaded = 0;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            setDownloadTotal(event.data.contentLength ?? 0);
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            setDownloadProgress(downloaded);
            break;
          case 'Finished':
            break;
        }
      });

      // Installed — force reboot to apply changes
      setState('reboot_required');
    } catch (err) {
      console.error('[UpdateGate] Auto-update failed:', err);
      setUpdateError(String(err));
      updateStartedRef.current = false;
    }
  }, []);

  const checkConnection = useCallback(async (appVersion: string): Promise<GateState> => {
    try {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 8000);

      const response = await fetch(API_VERSION_URL, {
        signal: controller.signal,
        cache: 'no-store',
      });
      clearTimeout(timeoutId);

      if (!response.ok) {
        return 'offline';
      }

      const data: VersionInfo = await response.json();
      setVersionInfo(data);

      // Kill switch takes absolute priority — no bypass
      if (data.kill_switch) {
        return 'killed';
      }

      // Check if update is available — use runtime version, not hardcoded
      const isBelowMin = compareSemver(appVersion, data.min_supported) < 0;
      const isOutdated = compareSemver(appVersion, data.latest) < 0;

      if (isBelowMin || isOutdated) {
        return 'updating';
      }

      return 'ok';
    } catch {
      return 'offline';
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function initialCheck() {
      // Read version from Tauri binary at runtime — never hardcoded
      let appVersion = '0.0.0';
      try {
        appVersion = await getVersion();
      } catch {
        // Fallback if Tauri API not available (shouldn't happen in production)
        appVersion = '0.0.0';
      }
      if (cancelled) return;

      const result = await checkConnection(appVersion);
      if (cancelled) return;
      setState(result);

      // performAutoUpdate is triggered reactively by the useEffect below
      // when both state === 'updating' and versionInfo are set

      // If offline, start retry loop
      if (result === 'offline') {
        startRetryLoop(appVersion);
      }
    }

    function startRetryLoop(appVersion: string) {
      if (retryTimerRef.current) return;
      retryTimerRef.current = setInterval(async () => {
        const result = await checkConnection(appVersion);
        if (cancelled) return;
        setState(result);

        if (result !== 'offline' && retryTimerRef.current) {
          clearInterval(retryTimerRef.current);
          retryTimerRef.current = null;
        }
      }, CONNECTIVITY_CHECK_INTERVAL);
    }

    initialCheck();

    return () => {
      cancelled = true;
      if (retryTimerRef.current) {
        clearInterval(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [checkConnection, performAutoUpdate]);

  // When state becomes 'updating' and we have versionInfo, trigger auto-update
  useEffect(() => {
    if (state === 'updating' && versionInfo && !updateStartedRef.current) {
      performAutoUpdate(versionInfo);
    }
  }, [state, versionInfo, performAutoUpdate]);

  /* ── Checking state ── */
  if (state === 'checking') {
    return (
      <div style={containerStyle}>
        <div style={innerStyle}>
          <span style={pulseStyle} />
          <span style={labelStyle}>CONNECTING...</span>
        </div>
      </div>
    );
  }

  /* ── Killed state (kill switch active) ── */
  if (state === 'killed') {
    return (
      <div style={installerContainerStyle}>
        <div style={installerContentStyle}>
          <div style={installerTypographyStyle}>
            <h1 style={installerHeadingStyle}>Service suspended.</h1>
            <p style={installerSubHeadingStyle}>
              {versionInfo?.kill_message || 'VanySound is temporarily unavailable. Hang tight — we\'re working on it.'}
            </p>
            <p style={installerDetailStyle}>
              This app cannot be used right now.
            </p>
          </div>
        </div>
      </div>
    );
  }

  /* ── Offline state ── */
  if (state === 'offline') {
    return (
      <div style={containerStyle}>
        <div style={cardStyle}>
          <div style={iconRowStyle}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#ff003c" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="1" y1="1" x2="23" y2="23" />
              <path d="M16.72 11.06A10.94 10.94 0 0119 12.55" />
              <path d="M5 12.55a10.94 10.94 0 015.17-2.39" />
              <path d="M10.71 5.05A16 16 0 0122.56 9" />
              <path d="M1.42 9a15.91 15.91 0 014.7-2.88" />
              <path d="M8.53 16.11a6 6 0 016.95 0" />
              <line x1="12" y1="20" x2="12.01" y2="20" />
            </svg>
          </div>

          <h1 style={titleStyle}>NO CONNECTION</h1>

          <p style={subtitleStyle}>
            VanySound requires an active internet connection to run.
            Check your network and we'll reconnect automatically.
          </p>

          <div style={retryIndicatorStyle}>
            <span style={pulseStyleSmall} />
            <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.3)', textTransform: 'uppercase' as const, letterSpacing: 1 }}>
              Retrying every 15 seconds...
            </span>
          </div>
        </div>
      </div>
    );
  }

  /* ── Updating state (auto-download + install) ── */
  if (state === 'updating') {
    const progressPercent = downloadTotal > 0
      ? Math.min(100, Math.round((downloadProgress / downloadTotal) * 100))
      : 0;

    // If auto-update errored, show fallback manual download
    if (updateError && versionInfo) {
      return (
        <div style={containerStyle}>
          <div style={cardStyle}>
            <div style={iconRowStyle}>
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#eeff00" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 9v4M12 17h.01" />
                <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
              </svg>
            </div>
            <h1 style={titleStyle}>UPDATE REQUIRED</h1>
            <p style={subtitleStyle}>
              Auto-update failed. Download <span style={{ color: '#eeff00', fontWeight: 800 }}>v{versionInfo.latest}</span> manually.
            </p>
            <a
              href={versionInfo.download_url}
              target="_blank"
              rel="noopener noreferrer"
              style={buttonStyle}
            >
              DOWNLOAD v{versionInfo.latest}
            </a>
            <p style={{ ...footerStyle, marginTop: 12, fontSize: 10, color: 'rgba(255,255,255,0.15)' }}>
              {updateError}
            </p>
          </div>
        </div>
      );
    }

    return (
      <div style={containerStyle}>
        <div style={cardStyle}>
          <div style={iconRowStyle}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#eeff00" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
              <polyline points="7 10 12 15 17 10" />
              <line x1="12" y1="15" x2="12" y2="3" />
            </svg>
          </div>

          <h1 style={titleStyle}>UPDATING</h1>

          <p style={subtitleStyle}>
            Installing <span style={{ color: '#eeff00', fontWeight: 800 }}>v{versionInfo?.latest}</span>...
            {'\n'}Don't close the app.
          </p>

          {/* Progress bar */}
          <div style={progressBarContainerStyle}>
            <div style={{ ...progressBarFillStyle, width: `${progressPercent}%` }} />
          </div>

          <p style={{ ...footerStyle, marginTop: 12 }}>
            {downloadTotal > 0
              ? `${(downloadProgress / 1048576).toFixed(1)} / ${(downloadTotal / 1048576).toFixed(1)} MB`
              : 'Preparing download...'
            }
          </p>
        </div>
      </div>
    );
  }

  /* ── Reboot required (post-update or post-install) ── */
  if (state === 'reboot_required') {
    return (
      <div style={containerStyle}>
        <div style={cardStyle}>
          <div style={iconRowStyle}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#eeff00" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 2v6h-6" />
              <path d="M3 12a9 9 0 0115-6.7L21 8" />
              <path d="M3 22v-6h6" />
              <path d="M21 12a9 9 0 01-15 6.7L3 16" />
            </svg>
          </div>

          <h1 style={titleStyle}>REBOOT REQUIRED</h1>

          <p style={subtitleStyle}>
            Update installed. Your PC needs to restart to apply audio changes.
          </p>

          <button
            style={buttonStyle}
            onClick={async () => {
              try {
                await invoke('reboot_system');
              } catch (err) {
                console.error('Reboot failed:', err);
              }
            }}
          >
            REBOOT NOW
          </button>

          <p style={{ ...footerStyle, marginTop: 16 }}>
            Your PC will restart in a few seconds.
          </p>
        </div>
      </div>
    );
  }

  /* ── Online + current version → let through ── */
  return <>{children}</>;
}

/* ── Inline styles ── */
const containerStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  zIndex: 99999,
  background: '#050505',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontFamily: "'JetBrains Mono', 'Consolas', monospace",
};

const innerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 12,
};

const pulseStyle: React.CSSProperties = {
  width: 8,
  height: 8,
  borderRadius: '50%',
  background: '#eeff00',
  boxShadow: '0 0 12px #eeff00',
  animation: 'pulse 1.5s ease-in-out infinite',
};

const pulseStyleSmall: React.CSSProperties = {
  width: 6,
  height: 6,
  borderRadius: '50%',
  background: '#ff003c',
  boxShadow: '0 0 8px #ff003c',
  animation: 'pulse 1.5s ease-in-out infinite',
};

const labelStyle: React.CSSProperties = {
  fontSize: 11,
  color: 'rgba(255,255,255,0.3)',
  textTransform: 'uppercase',
  letterSpacing: 2,
};

const cardStyle: React.CSSProperties = {
  maxWidth: 480,
  width: '90%',
  background: 'rgba(255,255,255,0.03)',
  border: '1px solid rgba(255,255,255,0.08)',
  borderRadius: 12,
  padding: '48px 40px',
  textAlign: 'center',
};

const iconRowStyle: React.CSSProperties = {
  marginBottom: 24,
};

const titleStyle: React.CSSProperties = {
  fontSize: 28,
  fontWeight: 900,
  color: '#fff',
  letterSpacing: '-0.03em',
  margin: '0 0 16px',
};

const subtitleStyle: React.CSSProperties = {
  fontSize: 14,
  color: 'rgba(255,255,255,0.6)',
  lineHeight: 1.7,
  margin: '0 0 24px',
};

const retryIndicatorStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 8,
  marginTop: 16,
};

const buttonStyle: React.CSSProperties = {
  display: 'inline-block',
  background: '#eeff00',
  color: '#000',
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 900,
  fontSize: 16,
  padding: '16px 40px',
  borderRadius: 8,
  textDecoration: 'none',
  textTransform: 'uppercase',
  cursor: 'pointer',
  transition: 'all 0.2s ease',
  boxShadow: '0 0 20px rgba(238,255,0,0.2)',
};

const progressBarContainerStyle: React.CSSProperties = {
  width: '100%',
  height: 6,
  background: 'rgba(255,255,255,0.06)',
  borderRadius: 999,
  overflow: 'hidden',
  marginTop: 8,
};

const progressBarFillStyle: React.CSSProperties = {
  height: '100%',
  background: 'linear-gradient(90deg, #eeff00, #c8f000)',
  borderRadius: 999,
  transition: 'width 0.15s ease-out',
  boxShadow: '0 0 12px rgba(238,255,0,0.3)',
};

const footerStyle: React.CSSProperties = {
  fontSize: 11,
  color: 'rgba(255,255,255,0.2)',
  marginTop: 20,
  textTransform: 'uppercase',
  letterSpacing: 1,
};

/* ── Installer-inspired kill switch styles ── */
const installerContainerStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  zIndex: 99999,
  background: 'radial-gradient(circle at 50% 0%, rgba(255, 255, 255, 0.03) 0%, rgba(0, 0, 0, 1) 70%)',
  backgroundColor: '#050505',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontFamily: "'Inter', sans-serif",
  overflow: 'hidden',
};

const installerContentStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  padding: 40,
  maxWidth: 600,
  margin: '0 auto',
  width: '100%',
  animation: 'cinematic-fade-up 0.8s cubic-bezier(0.16, 1, 0.3, 1) forwards',
};

const installerTypographyStyle: React.CSSProperties = {
  textAlign: 'left',
  width: '100%',
  marginBottom: 40,
  display: 'flex',
  flexDirection: 'column',
  gap: '6px',
};

const installerHeadingStyle: React.CSSProperties = {
  fontFamily: "var(--font-display, 'Inter', sans-serif)",
  fontSize: 34,
  letterSpacing: '-0.8px',
  color: '#FFF',
  fontWeight: 500,
  margin: 0,
  textShadow: '0 4px 20px rgba(255, 255, 255, 0.1)',
};

const installerSubHeadingStyle: React.CSSProperties = {
  fontFamily: "var(--font-body, 'Inter', sans-serif)",
  fontSize: 15,
  color: 'rgba(255, 255, 255, 0.6)',
  fontWeight: 400,
  margin: 0,
  lineHeight: 1.5,
};

const installerDetailStyle: React.CSSProperties = {
  fontSize: 13,
  fontFamily: "var(--font-mono, 'JetBrains Mono', monospace)",
  color: 'rgba(255, 255, 255, 0.3)',
  marginTop: 12,
  height: 20,
  transition: 'color 0.3s',
};
