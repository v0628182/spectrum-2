/**
 * useAuth — React hook for global auth state management
 *
 * Handles:
 *   - Session restoration on app startup (auto-login from localStorage)
 *   - Real-time auth state subscription (SIGNED_IN, SIGNED_OUT, TOKEN_REFRESHED)
 *   - Provides isLoading state for startup splash
 */
import { useEffect, useState, useCallback } from 'react';
import * as authService from './authService';
import { syncUserAfterLogin, type SyncResult } from './syncUser';
import type { Session, User } from '@supabase/supabase-js';

export interface AuthState {
  user: User | null;
  session: Session | null;
  syncResult: SyncResult | null;
  isAuthenticated: boolean;
  isLoading: boolean;
}

export function useAuth(): AuthState & {
  handleLogin: (user: User, session: Session) => Promise<boolean>;
  handleLogout: () => Promise<void>;
} {
  const [user, setUser] = useState<User | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  /* ── Restore session on mount ── */
  useEffect(() => {
    let cancelled = false;

    async function restoreSession() {
      try {
        const existingSession = await authService.getSession();
        if (cancelled) return;

        if (existingSession) {
          const authUser = await authService.getUser();
          if (cancelled || !authUser) {
            setIsLoading(false);
            return;
          }

          const sync = await syncUserAfterLogin(authUser);
          if (cancelled) return;

          if (sync.isBanned) {
            console.warn('[auth] Banned user detected during session restore — signing out');
            await authService.signOut();
            setIsLoading(false);
            return;
          }

          setUser(authUser);
          setSession(existingSession);
          setSyncResult(sync);
        }
      } catch (error) {
        console.error('[auth] Session restore failed:', error);
      } finally {
        if (!cancelled) setIsLoading(false);
      }
    }

    restoreSession();

    /* ── Subscribe to auth state changes ── */
    const { unsubscribe } = authService.onAuthStateChange(async (event, newSession) => {
      if (cancelled) return;

      if (event === 'SIGNED_IN' && newSession) {
        /* Discord OAuth (or any provider) just completed */
        const authUser = newSession.user;
        const sync = await syncUserAfterLogin(authUser);
        if (cancelled) return;

        if (sync.isBanned) {
          await authService.signOut();
          return;
        }

        setUser(authUser);
        setSession(newSession);
        setSyncResult(sync);
        setIsLoading(false);
      } else if (event === 'SIGNED_OUT') {
        setUser(null);
        setSession(null);
        setSyncResult(null);
      } else if (event === 'TOKEN_REFRESHED' && newSession) {
        setSession(newSession);
      }
    });

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, []);

  /** Called after successful signIn/signUp in AuthHUD */
  const handleLogin = useCallback(async (authUser: User, authSession: Session): Promise<boolean> => {
    const sync = await syncUserAfterLogin(authUser);

    if (sync.isBanned) {
      await authService.signOut();
      return false;
    }

    setUser(authUser);
    setSession(authSession);
    setSyncResult(sync);
    return true;
  }, []);

  /** Called from logout button or ban detection */
  const handleLogout = useCallback(async () => {
    await authService.signOut();
    setUser(null);
    setSession(null);
    setSyncResult(null);
  }, []);

  return {
    user,
    session,
    syncResult,
    isAuthenticated: user !== null && session !== null,
    isLoading,
    handleLogin,
    handleLogout,
  };
}
