/**
 * Supabase Client — Singleton
 *
 * Central entry point for all Supabase operations.
 * Uses the publishable anon key (safe to embed in client apps).
 * Session persistence via localStorage survives app restarts.
 *
 * flowType: 'pkce' enables secure OAuth for desktop apps where
 * the redirect can't hit the app directly. The code verifier is
 * stored in localStorage and exchanged after the user authorizes.
 */
import { createClient, type SupabaseClient } from '@supabase/supabase-js';

export const SUPABASE_URL = 'https://cnluoyurbnyhgaszvjxd.supabase.co';
const SUPABASE_ANON_KEY =
  'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImNubHVveXVyYm55aGdhc3p2anhkIiwicm9sZSI6ImFub24iLCJpYXQiOjE3NzY5MTc1NDgsImV4cCI6MjA5MjQ5MzU0OH0.c5J6bWsscKbg3ZFfTfYJOhqnnAwVa-EAbzpz2o419sc';

export const supabase: SupabaseClient = createClient(SUPABASE_URL, SUPABASE_ANON_KEY, {
  auth: {
    autoRefreshToken: true,
    persistSession: true,
    storage: window.localStorage,
    detectSessionInUrl: true,  // Auto-detect OAuth tokens/codes in URL on redirect
    flowType: 'pkce',          // Secure OAuth flow for desktop + web
  },
});
