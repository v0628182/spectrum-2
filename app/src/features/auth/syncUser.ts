/**
 * User Sync — Post-login synchronization with public.users table
 *
 * After successful Supabase Auth login, ensures:
 *   1. User exists in public.users (creates if first login)
 *   2. last_login_at timestamp is updated
 *   3. is_banned check — kicks banned users immediately
 *
 * Schema reference (vanysound project):
 *   - last_login_at: timestamptz
 *   - login_count: int (default 0)
 *   - subscription_tier: 'free' | 'starter' | 'pro' | 'elite'
 *   - subscription_status: 'active' | 'inactive' | 'cancelled' | 'past_due'
 *   - is_banned: boolean
 */
import { supabase } from '../../lib/supabase';
import type { User } from '@supabase/supabase-js';

export interface SyncResult {
  isBanned: boolean;
  isPremium: boolean;
  displayName: string;
  subscriptionTier: string;
}

/**
 * Sync authenticated user with public.users table.
 * Called once after every successful signIn/signUp.
 */
export async function syncUserAfterLogin(authUser: User): Promise<SyncResult> {
  const defaultResult: SyncResult = {
    isBanned: false,
    isPremium: false,
    displayName: authUser.user_metadata?.full_name ?? authUser.email?.split('@')[0] ?? 'User',
    subscriptionTier: 'free',
  };

  try {
    /* ── Step 1: Check if user exists in public.users ── */
    const { data: existingUser, error: selectError } = await supabase
      .from('users')
      .select('id, is_banned, subscription_tier, subscription_status, display_name, login_count')
      .eq('id', authUser.id)
      .maybeSingle();

    if (selectError) {
      console.warn('[auth:sync] Failed to query user record:', selectError.message);
      return defaultResult;
    }

    if (existingUser) {
      /* ── Step 2a: User exists — update last_login_at + increment login_count ── */
      await supabase
        .from('users')
        .update({
          last_login_at: new Date().toISOString(),
          login_count: (existingUser.login_count ?? 0) + 1,
        })
        .eq('id', authUser.id);

      const isPremium = existingUser.subscription_tier !== 'free'
        && existingUser.subscription_status === 'active';

      return {
        isBanned: existingUser.is_banned === true,
        isPremium,
        displayName: existingUser.display_name ?? defaultResult.displayName,
        subscriptionTier: existingUser.subscription_tier ?? 'free',
      };
    }

    /* ── Step 2b: First login — create public.users record ── */
    const { error: insertError } = await supabase.from('users').insert({
      id: authUser.id,
      email: authUser.email,
      display_name: defaultResult.displayName,
      last_login_at: new Date().toISOString(),
      login_count: 1,
    });

    if (insertError) {
      console.warn('[auth:sync] Failed to create user record:', insertError.message);
    }

    return defaultResult;
  } catch (unexpectedError) {
    console.error('[auth:sync] Unexpected error during sync:', unexpectedError);
    return defaultResult;
  }
}
