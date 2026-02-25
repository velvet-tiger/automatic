import { createContext, useContext } from "react";
import { useProfile, type UserProfile } from "./useProfile";

interface ProfileContextValue {
  profile: UserProfile | null;
  userId: string | null;
  isLoaded: boolean;
}

const ProfileContext = createContext<ProfileContextValue>({
  profile: null,
  userId: null,
  isLoaded: false,
});

/**
 * Provides the authenticated user's profile to the entire component tree.
 * Must be rendered inside Clerk's `<SignedIn>` boundary so useUser() works.
 */
export function ProfileProvider({ children }: { children: React.ReactNode }) {
  const value = useProfile();
  return (
    <ProfileContext.Provider value={value}>{children}</ProfileContext.Provider>
  );
}

/**
 * Returns the current user's profile context.
 * - `userId` is the Clerk user ID (or null if not loaded / signed out).
 * - Use `userId` when setting `created_by` on projects, memory, etc.
 */
export function useCurrentUser() {
  return useContext(ProfileContext);
}
