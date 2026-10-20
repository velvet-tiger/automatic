/**
 * Env values that start with Automatic's encryption prefix.
 *
 * The backend stores a value with this prefix as it is, because it takes it
 * for ciphertext. A plain value with the prefix could never be decrypted, so
 * the backend refuses it from the editor and blanks it in imported config.
 * These helpers let the UI do the same before it calls the backend. The
 * backend check stays the authority.
 */

/** Must match `SENTINEL` in src-tauri/src/core/env_crypto.rs. */
export const ENCRYPTED_VALUE_PREFIX = "enc:v1:";

/** The keys of every string value in `env` that starts with the prefix. */
export function envKeysWithEncryptedPrefix(env: unknown): string[] {
  if (typeof env !== "object" || env === null || Array.isArray(env)) return [];
  return Object.entries(env as Record<string, unknown>)
    .filter(([, value]) => typeof value === "string" && value.startsWith(ENCRYPTED_VALUE_PREFIX))
    .map(([key]) => key);
}

/** The message the backend gives when it refuses such a value. */
export function encryptedPrefixMessage(key: string): string {
  return `Env value for '${key}' cannot start with '${ENCRYPTED_VALUE_PREFIX}'. That prefix is reserved for encrypted values.`;
}

/**
 * Return a copy of `config` with every prefixed `env` value set to an empty
 * string, and the keys that were cleared. An empty value means "inherit from
 * the environment", which is what the backend stores for imported config.
 * `config` itself is not changed.
 */
export function blankEncryptedPrefixEnvValues(config: Record<string, unknown>): {
  config: Record<string, unknown>;
  clearedKeys: string[];
} {
  const clearedKeys = envKeysWithEncryptedPrefix(config.env);
  if (clearedKeys.length === 0) return { config, clearedKeys };
  const env = { ...(config.env as Record<string, unknown>) };
  for (const key of clearedKeys) env[key] = "";
  return { config: { ...config, env }, clearedKeys };
}
