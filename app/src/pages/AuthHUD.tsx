import React, { useState, useCallback } from 'react';
import { CustomTitlebar } from '../widgets/CustomTitlebar';
import * as authService from '../features/auth/authService';
import type { User, Session } from '@supabase/supabase-js';
import './auth-hud.css';

/* ── Inline SVG logo — avoids Tauri asset path issues in prod builds ── */
const VanySoundLogo: React.FC<{ size?: number }> = ({ size = 80 }) => (
  <svg width={size} height={size * (434/535)} viewBox="0 0 535 434" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path d="M165.679 286.156L126.881 222.019L80.6584 144.888L0 7.61103L96.3446 7.42539L170.227 129.759L262.024 279.473L287.456 318.457C300.357 304.72 319.663 267.314 308.897 250.143C304.441 245.317 299.429 241.79 294.417 237.613C266.757 214.965 241.14 195.752 227.403 161.502C211.809 122.519 218.028 78.5236 244.945 45.9447C268.243 17.7281 302.678 0.464087 339.898 0.37127L535 0C529.524 14.2939 523.212 26.3602 514.023 37.4055C495.553 60.7026 469.007 74.9037 438.934 74.9965L341.011 75.3678C328.945 75.3678 318.271 79.823 310.01 88.1766C298.779 99.4075 296.737 115.743 304.07 129.759C307.597 136.535 312.516 141.918 317.9 147.58L368.3 200.022C404.406 237.613 393.546 299.8 358.925 334.421C346.58 333.772 335.442 336.092 324.026 339.898C308.247 346.209 294.974 355.584 282.629 367.372C267.407 381.573 255.434 398.187 244.853 416.565L165.679 286.156Z" fill="#D2D800"/>
    <path d="M255.673 433.586C269.874 405.555 289.737 378.267 318.51 364.901C327.421 360.724 336.517 358.218 346.356 358.125L378.192 357.847C401.582 357.661 420.981 343.089 431.191 322.019C442.7 298.351 440.194 271.341 427.756 248.137C420.238 234.028 411.049 221.869 400.004 210.081L353.595 160.424C369.281 160.424 384.039 159.589 399.726 160.424C448.826 163.116 490.594 194.766 508.786 239.969C516.676 259.646 519.553 279.88 518.439 300.95C514.819 373.347 456.251 432.936 383.39 433.122L255.58 433.493L255.673 433.586Z" fill="#E9E8E8"/>
  </svg>
);

/* ═══════════════════════════════════════════════════════════
   Types
   ═══════════════════════════════════════════════════════════ */

interface AuthHUDProps {
  onLogin: (user: User, session: Session) => Promise<boolean>;
}

type ViewState = 'login' | 'forgot' | 'signup';

interface ErrorState {
  message: string;
  visible: boolean;
}

/* ═══════════════════════════════════════════════════════════
   Sub-Components
   ═══════════════════════════════════════════════════════════ */

const ErrorBanner: React.FC<{ error: ErrorState }> = ({ error }) => {
  if (!error.visible) return null;
  return (
    <div className="auth-error-banner">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="8" x2="12" y2="12" />
        <line x1="12" y1="16" x2="12.01" y2="16" />
      </svg>
      <span>{error.message}</span>
    </div>
  );
};

const LockoutWarning: React.FC<{ delayMs: number }> = ({ delayMs }) => {
  if (delayMs <= 0) return null;
  const seconds = Math.ceil(delayMs / 1000);
  return (
    <div className="auth-lockout-warning">
      [ THROTTLED ] TOO MANY ATTEMPTS. WAIT {seconds}S.
    </div>
  );
};

const Spinner: React.FC = () => <div className="auth-spinner" />;

/* ═══════════════════════════════════════════════════════════
   Shared SVG
   ═══════════════════════════════════════════════════════════ */

const DiscordIcon = () => (
  <svg width="18" height="18" fill="#5865F2" viewBox="0 0 24 24">
    <path d="M20.3 4.4C18.8 3.7 17.2 3.2 15.5 3c-.2.4-.5.8-.7 1.2-1.8-.3-3.6-.3-5.5 0-.2-.4-.4-.8-.7-1.2-1.7.2-3.3.7-4.8 1.4-3 4.5-3.8 8.8-3.4 13.1 2 1.5 3.9 2.4 5.8 3 .5-.6.9-1.3 1.2-2.1-.7-.3-1.3-.6-1.9-1 .1-.1.3-.2.4-.3 3.8 1.8 7.9 1.8 11.6 0 .1.1.3.2.4.3-.6.4-1.2.7-1.9 1 .3.8.7 1.5 1.2 2.1 1.9-.6 3.8-1.5 5.8-3 .4-4.3-.4-8.6-3.4-13.1zM9 14.6c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm6 0c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z" />
  </svg>
);

/* ═══════════════════════════════════════════════════════════
   Main Component
   ═══════════════════════════════════════════════════════════ */

export const AuthHUD: React.FC<AuthHUDProps> = ({ onLogin }) => {
  const [view, setView] = useState<ViewState>('login');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ErrorState>({ message: '', visible: false });
  const [toast, setToast] = useState<{ msg: string; visible: boolean }>({ msg: '', visible: false });
  const [showPassword, setShowPassword] = useState(false);
  const [forgotSuccess, setForgotSuccess] = useState(false);
  const [forgotEmail, setForgotEmail] = useState('');

  /* ── Form refs ── */
  const [loginEmail, setLoginEmail] = useState('');
  const [loginPassword, setLoginPassword] = useState('');
  const [signupEmail, setSignupEmail] = useState('');
  const [signupPassword, setSignupPassword] = useState('');
  const [resetEmail, setResetEmail] = useState('');

  const showError = useCallback((message: string) => {
    setError({ message, visible: true });
  }, []);

  const clearError = useCallback(() => {
    setError({ message: '', visible: false });
  }, []);

  const showToast = useCallback((msg: string) => {
    setToast({ msg, visible: true });
    setTimeout(() => setToast({ msg, visible: false }), 3500);
  }, []);

  const switchView = useCallback((nextView: ViewState) => {
    clearError();
    setShowPassword(false);
    setForgotSuccess(false);
    setView(nextView);
  }, [clearError]);

  /* ═══════════════════════════════════════
     LOGIN HANDLER — Real Supabase Auth
     ═══════════════════════════════════════ */
  const handleLogin = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;
    clearError();

    const email = loginEmail.trim();
    const password = loginPassword;

    if (!email || !password) {
      showError('ALL FIELDS REQUIRED.');
      return;
    }

    setLoading(true);

    const result = await authService.signIn(email, password);

    if (!result.success || !result.user || !result.session) {
      showError(result.error ?? 'AUTHENTICATION FAILED.');
      setLoading(false);
      return;
    }

    /* Sync user and check ban status */
    const allowed = await onLogin(result.user, result.session);
    if (!allowed) {
      showError('YOUR ACCOUNT HAS BEEN SUSPENDED. CONTACT SUPPORT.');
      setLoading(false);
      return;
    }

    /* Success — App.tsx will handle the transition */
    setLoading(false);
  }, [loading, loginEmail, loginPassword, clearError, showError, onLogin]);

  /* ═══════════════════════════════════════
     SIGNUP HANDLER — Real Supabase Auth
     ═══════════════════════════════════════ */
  const handleSignup = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;
    clearError();

    const email = signupEmail.trim();
    const password = signupPassword;

    if (!email || !password) {
      showError('ALL FIELDS REQUIRED.');
      return;
    }

    setLoading(true);
    const result = await authService.signUp(email, password);

    if (result.error) {
      showError(result.error);
      setLoading(false);
      return;
    }

    if (result.requiresConfirmation) {
      showToast('CHECK YOUR EMAIL TO CONFIRM YOUR ACCOUNT.');
      switchView('login');
      setLoading(false);
      return;
    }

    if (result.success && result.user && result.session) {
      const allowed = await onLogin(result.user, result.session);
      if (!allowed) {
        showError('YOUR ACCOUNT HAS BEEN SUSPENDED.');
        setLoading(false);
        return;
      }
    }

    setLoading(false);
  }, [loading, signupEmail, signupPassword, clearError, showError, showToast, switchView, onLogin]);

  /* ═══════════════════════════════════════
     FORGOT PASSWORD — Blind reset (C9-FIX)
     ═══════════════════════════════════════ */
  const handleForgot = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;
    clearError();

    const email = resetEmail.trim();
    if (!email) {
      showError('ENTER YOUR EMAIL ADDRESS.');
      return;
    }

    setLoading(true);
    await authService.resetPassword(email);
    setForgotEmail(email);
    setForgotSuccess(true);
    setLoading(false);
  }, [loading, resetEmail, clearError, showError]);

  /* ═══════════════════════════════════════
     DISCORD OAuth — Webview redirect flow.
     signInWithOAuth navigates to Discord →
     Supabase handles PKCE → onAuthStateChange
     fires SIGNED_IN → useAuth picks it up.
     ═══════════════════════════════════════ */
  const handleDiscord = useCallback(async () => {
    if (loading) return;
    clearError();

    try {
      setLoading(true);
      await authService.signInWithDiscord();
      /* Webview navigates away — Supabase handles the rest */
    } catch {
      showError('DISCORD AUTH FAILED. TRY EMAIL INSTEAD.');
      setLoading(false);
    }
  }, [loading, clearError, showError]);

  const throttleMs = authService.getThrottleDelayMs();

  return (
    <>
    <div className="optimizer-container" style={{ background: 'var(--bg-base)' }}>
      <CustomTitlebar />

      <div className="auth-wrapper">
        <div className="auth-card">

          {/* ══════════════════════════════════
              LOGIN VIEW
              ══════════════════════════════════ */}
          {view === 'login' && (
            <div className="auth-view fade-in">
              <div style={{ marginBottom: 32, display: 'flex', justifyContent: 'center' }}>
                <VanySoundLogo size={80} />
              </div>
              <h2>Get In.</h2>
              <p className="auth-sub">Drop your credentials and let's get to work.</p>

              <ErrorBanner error={error} />
              <LockoutWarning delayMs={throttleMs} />

              <form onSubmit={handleLogin} noValidate>
                <div className="auth-group">
                  <label>Email</label>
                  <input
                    type="email"
                    placeholder="operator@vanysound.com"
                    required
                    disabled={loading}
                    value={loginEmail}
                    onChange={(e) => setLoginEmail(e.target.value)}
                    autoComplete="email"
                  />
                </div>
                <div className="auth-group">
                  <div className="auth-split">
                    <label>Password</label>
                    <a href="#" onClick={(e) => { e.preventDefault(); switchView('forgot'); }}>Locked out?</a>
                  </div>
                  <div className="auth-password-wrapper">
                    <input
                      type={showPassword ? 'text' : 'password'}
                      placeholder="••••••••"
                      required
                      disabled={loading}
                      value={loginPassword}
                      onChange={(e) => setLoginPassword(e.target.value)}
                      autoComplete="current-password"
                    />
                    <button
                      type="button"
                      className="auth-toggle-pw"
                      onClick={() => setShowPassword(!showPassword)}
                      tabIndex={-1}
                    >
                      {showPassword ? 'HIDE' : 'SHOW'}
                    </button>
                  </div>
                </div>
                <button type="submit" className="auth-btn btn-primary" disabled={loading}>
                  {loading ? <Spinner /> : 'Log In'}
                </button>
              </form>

              <div className="auth-divider"><span>or continue with</span></div>

              <button className="auth-btn btn-discord" disabled={loading} onClick={handleDiscord}>
                <DiscordIcon />
                Discord
              </button>

              <div className="auth-footer">
                No account? <a href="#" onClick={(e) => { e.preventDefault(); switchView('signup'); }}>Get one.</a>
              </div>


            </div>
          )}

          {/* ══════════════════════════════════
              FORGOT PASSWORD VIEW
              ══════════════════════════════════ */}
          {view === 'forgot' && (
            <div className="auth-view fade-in">
              <div style={{ marginBottom: 32, display: 'flex', justifyContent: 'center' }}>
                <VanySoundLogo size={80} />
              </div>
              <h2>Locked Out?</h2>
              <p className="auth-sub">Drop your email. We'll send you the keys.</p>

              <ErrorBanner error={error} />

              {forgotSuccess ? (
                <div className="auth-forgot-success fade-in">
                  <div className="auth-success-icon">✓</div>
                  <h3>Recovery Sent.</h3>
                  <p>If an account exists for <strong>{forgotEmail}</strong>, you'll get a reset link shortly.</p>
                  <button className="auth-link-btn" onClick={() => switchView('login')}>← BACK TO SIGN IN</button>
                </div>
              ) : (
                <form onSubmit={handleForgot} noValidate>
                  <div className="auth-group">
                    <label>Email</label>
                    <input
                      type="email"
                      placeholder="name@example.com"
                      required
                      disabled={loading}
                      value={resetEmail}
                      onChange={(e) => setResetEmail(e.target.value)}
                      autoComplete="email"
                    />
                  </div>
                  <button type="submit" className="auth-btn btn-primary" disabled={loading}>
                    {loading ? <Spinner /> : 'Send Recovery Link'}
                  </button>
                </form>
              )}

              <div className="auth-footer">
                Remembered your password? <a href="#" onClick={(e) => { e.preventDefault(); switchView('login'); }}>Sign in</a>
              </div>
            </div>
          )}

          {/* ══════════════════════════════════
              SIGNUP VIEW
              ══════════════════════════════════ */}
          {view === 'signup' && (
            <div className="auth-view fade-in">
              <div style={{ marginBottom: 32, display: 'flex', justifyContent: 'center' }}>
                <VanySoundLogo size={80} />
              </div>
              <h2>Join the Elite.</h2>
              <p className="auth-sub">Stop playing like a rookie. Claim your license.</p>

              <ErrorBanner error={error} />

              <form onSubmit={handleSignup} noValidate>
                <div className="auth-group">
                  <label>Email</label>
                  <input
                    type="email"
                    placeholder="operator@vanysound.com"
                    required
                    disabled={loading}
                    value={signupEmail}
                    onChange={(e) => setSignupEmail(e.target.value)}
                    autoComplete="email"
                  />
                </div>
                <div className="auth-group">
                  <label>Password (min 8 chars, 1 uppercase, 1 number)</label>
                  <div className="auth-password-wrapper">
                    <input
                      type={showPassword ? 'text' : 'password'}
                      placeholder="Make it tough."
                      minLength={8}
                      required
                      disabled={loading}
                      value={signupPassword}
                      onChange={(e) => setSignupPassword(e.target.value)}
                      autoComplete="new-password"
                    />
                    <button
                      type="button"
                      className="auth-toggle-pw"
                      onClick={() => setShowPassword(!showPassword)}
                      tabIndex={-1}
                    >
                      {showPassword ? 'HIDE' : 'SHOW'}
                    </button>
                  </div>
                </div>
                <button type="submit" className="auth-btn btn-primary" disabled={loading}>
                  {loading ? <Spinner /> : 'Lock It In'}
                </button>
              </form>

              <div className="auth-divider"><span>or continue with</span></div>
              <button className="auth-btn btn-discord" disabled={loading} onClick={handleDiscord}>
                <DiscordIcon />
                Discord
              </button>

              <div className="auth-footer">
                Already have an account? <a href="#" onClick={(e) => { e.preventDefault(); switchView('login'); }}>Log in</a>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className={`auth-toast ${toast.visible ? 'show' : ''}`}>
        {toast.msg}
      </div>
    </div>
    </>
  );
};
