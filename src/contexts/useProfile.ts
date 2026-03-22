import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface UserProfile {
  clerk_id: string;
  email: string;
  display_name: string;
  avatar_url: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Reads the locally stored user profile from the Tauri backend.
 *
 * Authentication has been removed from the desktop app. The profile model
 * is preserved for future web-service authorisation — `clerk_id` will be
 * populated once a web service issues a token and syncs the profile.
 *
 * Returns the current local profile (or null while loading / not yet set).
 * Also exposes `userId` (the stored `clerk_id`) for tagging content with
 * `created_by`.
 */
export function useProfile() {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    const loadProfile = async () => {
      try {
        const saved: UserProfile | null = await invoke("read_profile");
        setProfile(saved);
      } catch (e) {
        console.error("[useProfile] Failed to read profile:", e);
      } finally {
        setIsLoaded(true);
      }
    };

    loadProfile();
  }, []);

  return {
    profile,
    /** The stored user ID (clerk_id), or null if no profile has been saved yet. */
    userId: profile?.clerk_id ?? null,
    isLoaded,
  };
}
