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
 * Provides the locally stored user profile to the entire component tree.
 */
export function ProfileProvider({ children }: { children: React.ReactNode }) {
  const value = useProfile();
  return (
    <ProfileContext.Provider value={value}>{children}</ProfileContext.Provider>
  );
}

/**
 * Returns the current user's profile context.
 * - `userId` is the stored user ID (or null if no profile has been set up yet).
 * - Use `userId` when setting `created_by` on projects, memory, etc.
 */
export function useCurrentUser() {
  return useContext(ProfileContext);
}
