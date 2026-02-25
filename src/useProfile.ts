import { useEffect, useState } from "react";
import { useUser } from "@clerk/clerk-react";
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
 * Extracts the Clerk user identity and persists it locally via the Tauri
 * backend.  On each authenticated session mount the profile is synced so
 * that display name / email / avatar changes propagate.
 *
 * Returns the current local profile (or null while loading / signed out).
 * Also exposes the Clerk user ID directly for convenience when tagging
 * content with `created_by`.
 */
export function useProfile() {
  const { isLoaded, isSignedIn, user } = useUser();
  const [profile, setProfile] = useState<UserProfile | null>(null);

  useEffect(() => {
    if (!isLoaded || !isSignedIn || !user) return;

    const syncProfile = async () => {
      const profileData: UserProfile = {
        clerk_id: user.id,
        email: user.primaryEmailAddress?.emailAddress ?? "",
        display_name:
          user.fullName ?? user.username ?? user.firstName ?? "",
        avatar_url: user.imageUrl ?? null,
        // created_at / updated_at are managed by the backend
        created_at: "",
        updated_at: "",
      };

      try {
        await invoke("save_profile", { profile: profileData });
        const saved: UserProfile | null = await invoke("read_profile");
        setProfile(saved);
      } catch (e) {
        console.error("[useProfile] Failed to sync profile:", e);
      }
    };

    syncProfile();
  }, [isLoaded, isSignedIn, user?.id, user?.primaryEmailAddress?.emailAddress, user?.fullName, user?.imageUrl]);

  return {
    profile,
    /** Shorthand: the Clerk user ID, or null if not yet loaded. */
    userId: profile?.clerk_id ?? null,
    isLoaded: isLoaded && profile !== null,
  };
}
