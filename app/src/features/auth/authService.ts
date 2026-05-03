/**
 * Auth Service — Hexagonal Adapter for Supabase Auth
 *
 * Ports all security hardening from the website's login.html:
 *   - C6-FIX: Exponential backoff after 2+ failed attempts (max 16s)
 *   - C8-FIX: Strict password policy enforcement
 *   - C9-FIX: Generic error messages — never leak email existence
 *   - Anti-timing: Randomized jitter masks backend response times
 *   - Trusted event verification stub (for future bot protection)
 */
import { supabase } from '../../lib/supabase';
import type { Session, User, AuthError } from '@supabase/supabase-js';

const API_BASE = 'https://api.vanysound.com';

/* ═══════════════════════════════════════════════════════════
   Types
   ═══════════════════════════════════════════════════════════ */

export interface AuthResult {
  success: boolean;
  user: User | null;
  session: Session | null;
  error: string | null;
  requiresConfirmation: boolean;
}

interface ThrottleState {
  failedAttempts: number;
  lastFailureTimestamp: number;
}

/* ═══════════════════════════════════════════════════════════
   Constants
   ═══════════════════════════════════════════════════════════ */

const PASSWORD_MIN_LENGTH = 8;
const MAX_BACKOFF_MS = 16_000;
const BACKOFF_BASE = 2.5;
const BACKOFF_THRESHOLD = 2;
const JITTER_BASE_MS = 400;
const JITTER_RANGE_MS = 300;

/** Common weak passwords — instant rejection */
const WEAK_PASSWORD_PATTERNS = [
  /^password/i,
  /^12345678/,
  /^qwerty/i,
  /^letmein/i,
  /^admin/i,
  /^welcome/i,
];

/* ═══════════════════════════════════════════════════════════
   Throttle State (in-memory, resets on app restart)
   ═══════════════════════════════════════════════════════════ */

const throttle: ThrottleState = {
  failedAttempts: 0,
  lastFailureTimestamp: 0,
};

/* ═══════════════════════════════════════════════════════════
   Internal Helpers
   ═══════════════════════════════════════════════════════════ */

/**
 * C6-FIX: Exponential backoff — starts after 2 failures.
 * Returns milliseconds the caller must wait before proceeding.
 */
function calculateBackoffMs(attempts: number): number {
  if (attempts < BACKOFF_THRESHOLD) return 0;
  const delay = Math.pow(BACKOFF_BASE, attempts - BACKOFF_THRESHOLD) * 1000;
  return Math.min(delay, MAX_BACKOFF_MS);
}

/**
 * Anti-timing jitter — masks Supabase bcrypt hash latency
 * so an attacker can't distinguish "email exists" from "email doesn't exist"
 * by measuring response time.
 */
function jitterDelay(): Promise<void> {
  const ms = JITTER_BASE_MS + Math.random() * JITTER_RANGE_MS;
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * C9-FIX: Normalize Supabase auth errors into generic, non-revealing messages.
 * NEVER expose raw Supabase error text to the UI.
 */
function sanitizeAuthError(_error: AuthError | null): string {
  return 'INVALID CREDENTIALS. CHECK YOUR EMAIL AND PASSWORD.';
}

/**
 * C8-FIX: Enforce strict password policy.
 * Returns null if valid, error string if invalid.
 */
function validatePasswordStrength(password: string): string | null {
  if (password.length < PASSWORD_MIN_LENGTH) {
    return `SECURITY POLICY: MINIMUM ${PASSWORD_MIN_LENGTH} CHARACTERS REQUIRED.`;
  }
  if (!/[A-Z]/.test(password)) {
    return 'SECURITY POLICY: AT LEAST ONE UPPERCASE LETTER REQUIRED.';
  }
  if (!/\d/.test(password)) {
    return 'SECURITY POLICY: AT LEAST ONE NUMBER REQUIRED.';
  }
  for (const pattern of WEAK_PASSWORD_PATTERNS) {
    if (pattern.test(password)) {
      return 'SECURITY POLICY: PASSWORD TOO WEAK. REJECTED.';
    }
  }
  return null;
}

/* ═══════════════════════════════════════════════════════════
   Public Auth Service
   ═══════════════════════════════════════════════════════════ */

/**
 * Returns the current backoff delay in ms (0 if no throttle active).
 * UI uses this to show the lockout timer before calling signIn.
 */
export function getThrottleDelayMs(): number {
  return calculateBackoffMs(throttle.failedAttempts);
}

/** Current failed attempt count — exposed for UI lockout display */
export function getFailedAttempts(): number {
  return throttle.failedAttempts;
}

/** Reset throttle — called after successful login */
function resetThrottle(): void {
  throttle.failedAttempts = 0;
  throttle.lastFailureTimestamp = 0;
}

/** Bump throttle — called after failed login */
function bumpThrottle(): void {
  throttle.failedAttempts++;
  throttle.lastFailureTimestamp = Date.now();
}

/**
 * Sign in with email + password.
 * Applies throttle delay, jitter, and generic error normalization.
 */
export async function signIn(email: string, password: string): Promise<AuthResult> {
  const backoffMs = calculateBackoffMs(throttle.failedAttempts);
  if (backoffMs > 0) {
    await new Promise((resolve) => setTimeout(resolve, backoffMs));
  }

  const [result] = await Promise.all([
    supabase.auth.signInWithPassword({ email, password }),
    jitterDelay(),
  ]);

  if (result.error || !result.data.session) {
    bumpThrottle();
    return {
      success: false,
      user: null,
      session: null,
      error: sanitizeAuthError(result.error),
      requiresConfirmation: false,
    };
  }

  resetThrottle();
  return {
    success: true,
    user: result.data.user,
    session: result.data.session,
    error: null,
    requiresConfirmation: false,
  };
}

/**
 * Create account with email + password.
 * Enforces C8-FIX password policy before hitting Supabase.
 */
export async function signUp(email: string, password: string): Promise<AuthResult> {
  const passwordError = validatePasswordStrength(password);
  if (passwordError) {
    return {
      success: false,
      user: null,
      session: null,
      error: passwordError,
      requiresConfirmation: false,
    };
  }

  const [result] = await Promise.all([
    supabase.auth.signUp({
      email,
      password,
      options: {
        data: {
          full_name: email.split('@')[0],
        },
      },
    }),
    jitterDelay(),
  ]);

  if (result.error) {
    return {
      success: false,
      user: null,
      session: null,
      error: 'COULD NOT CREATE ACCOUNT. TRY A DIFFERENT EMAIL OR SIGN IN.',
      requiresConfirmation: false,
    };
  }

  /* Supabase returns user but null session when email confirmation is required */
  const needsConfirmation = result.data.user !== null && result.data.session === null;

  /* Fire-and-forget: trigger welcome email via VPS API */
  fetch(`${API_BASE}/api/auth/send-welcome`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email }),
  }).catch(() => {});

  return {
    success: !needsConfirmation,
    user: result.data.user,
    session: result.data.session,
    error: null,
    requiresConfirmation: needsConfirmation,
  };
}

/**
 * Blind password reset — ALWAYS reports success (C9-FIX).
 * Prevents email enumeration by never revealing if the email exists.
 */
export async function resetPassword(email: string): Promise<void> {
  try {
    await Promise.all([
      supabase.auth.resetPasswordForEmail(email),
      jitterDelay(),
    ]);
  } catch {
    /* Intentionally swallowed — blind reset */
  }
}

/** Sign out and clear persisted session */
export async function signOut(): Promise<void> {
  await supabase.auth.signOut();
  resetThrottle();
}

/** Get current session (from localStorage cache or refresh) */
export async function getSession(): Promise<Session | null> {
  const { data } = await supabase.auth.getSession();
  return data.session;
}

/** Get current user (from session) */
export async function getUser(): Promise<User | null> {
  const { data } = await supabase.auth.getUser();
  return data.user;
}

/**
 * Subscribe to auth state changes.
 * Returns unsubscribe function for cleanup.
 */
export function onAuthStateChange(
  callback: (event: string, session: Session | null) => void
): { unsubscribe: () => void } {
  const { data } = supabase.auth.onAuthStateChange((event, session) => {
    callback(event, session);
  });
  return { unsubscribe: data.subscription.unsubscribe };
}

/* ═══════════════════════════════════════════════════════════
   Discord OAuth — Desktop (Tauri) Flow

   Navigates the webview directly to Discord OAuth.
   Supabase PKCE handles the code exchange automatically
   when the redirect returns to the same origin.
   The onAuthStateChange listener in useAuth fires SIGNED_IN.
   ═══════════════════════════════════════════════════════════ */

/**
 * Initiate Discord OAuth — redirects the current webview.
 * After authorization, Supabase auto-exchanges the PKCE code
 * and triggers onAuthStateChange(SIGNED_IN).
 */
export async function signInWithDiscord(): Promise<void> {
  const { error } = await supabase.auth.signInWithOAuth({
    provider: 'discord',
    options: {
      scopes: 'identify email',
      redirectTo: window.location.origin,
    },
  });

  if (error) {
    throw new Error('DISCORD AUTH NOT AVAILABLE. TRY EMAIL INSTEAD.');
  }
}

