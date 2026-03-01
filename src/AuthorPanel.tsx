import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink, HardDrive, User } from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

/**
 * Raw author descriptor — stored in item JSON as `_author`.
 * Mirrors the Rust `AuthorDescriptor` struct.
 */
export type AuthorDescriptor =
  | { type: "local" }
  | { type: "github"; repo: string; url?: string }
  | { type: "provider"; name: string; url?: string; repository_url?: string };

/**
 * Resolved author profile returned by the `resolve_author` Tauri command.
 * Mirrors the Rust `AuthorProfile` struct.
 */
export interface AuthorProfile {
  name: string;
  bio: string;
  avatar_url: string;
  url: string;
  kind: string;
}

// ── Module-level cache for GitHub profile lookups ─────────────────────────────
// Keyed by the full descriptor JSON string. Populated on first fetch per
// session (including fallbacks); cleared on page reload. Prevents redundant
// GitHub API calls — including when rate-limited — when the same author
// appears across multiple items.
const profileCache = new Map<string, AuthorProfile>();

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Derive initials from a display name for the avatar fallback. */
function initials(name: string): string {
  const words = name.trim().split(/\s+/);
  if (words.length === 1) return (words[0]![0] ?? "?").toUpperCase();
  return ((words[0]![0] ?? "") + (words[words.length - 1]![0] ?? "")).toUpperCase();
}

/** Consistent background colour for the initials avatar, derived from the name.
 *  Colors are read from the active theme's CSS variables so the avatar palette
 *  automatically matches whichever theme is applied to the document root. */
function avatarColor(name: string): string {
  const root = document.documentElement;
  const style = getComputedStyle(root);
  const palette = [
    style.getPropertyValue("--avatar-0").trim(),
    style.getPropertyValue("--avatar-1").trim(),
    style.getPropertyValue("--avatar-2").trim(),
    style.getPropertyValue("--avatar-3").trim(),
    style.getPropertyValue("--avatar-4").trim(),
    style.getPropertyValue("--avatar-5").trim(),
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) | 0;
  return palette[Math.abs(hash) % palette.length]!;
}

// ── Client-side resolution for non-network descriptor types ──────────────────

function resolveLocally(desc: AuthorDescriptor | null): AuthorProfile | null {
  if (!desc || desc.type === "local") {
    return { name: "You", bio: "Created locally", avatar_url: "", url: "", kind: "local" };
  }
  if (desc.type === "provider") {
    return {
      name: desc.name,
      bio: "",
      avatar_url: "",
      url: desc.url ?? desc.repository_url ?? "",
      kind: "provider",
    };
  }
  // github — derive an immediate profile from the repo string using the
  // GitHub CDN avatar (unauthenticated, not rate-limited). The Tauri call
  // may upgrade this with a real display name and bio, but the user sees
  // something meaningful immediately instead of a skeleton.
  if (desc.type === "github" && desc.repo) {
    const owner = desc.repo.split("/")[0] ?? desc.repo;
    return {
      name: owner,
      bio: desc.repo,
      avatar_url: `https://github.com/${owner}.png?size=80`,
      url: desc.url ?? `https://github.com/${owner}`,
      kind: "github",
    };
  }
  return null;
}

// ── Avatar ────────────────────────────────────────────────────────────────────

function Avatar({ url, name, size = 40 }: { url: string; name: string; size?: number }) {
  const [broken, setBroken] = useState(false);

  useEffect(() => {
    setBroken(false);
  }, [url]);

  if (url && !broken) {
    return (
      <img
        src={url}
        alt={name}
        width={size}
        height={size}
        className="rounded-full object-cover flex-shrink-0"
        style={{ width: size, height: size }}
        onError={() => setBroken(true)}
      />
    );
  }

  // Initials fallback
  const bg = avatarColor(name);
  return (
    <div
      className="rounded-full flex items-center justify-center flex-shrink-0 text-white font-semibold select-none"
      style={{ width: size, height: size, background: bg, fontSize: size * 0.38 }}
    >
      {name === "You" ? <User size={size * 0.5} strokeWidth={2} /> : initials(name)}
    </div>
  );
}

// ── AuthorPanel ───────────────────────────────────────────────────────────────

interface AuthorPanelProps {
  /**
   * Pass either a pre-built descriptor object or a JSON string (as stored in
   * item data).  When omitted, the panel shows the "local / You" state.
   */
  descriptor?: AuthorDescriptor | string | null;
}

type LoadState = "loading" | "ready";

export function AuthorPanel({ descriptor }: AuthorPanelProps) {
  // Normalise descriptor to an object
  const desc: AuthorDescriptor | null =
    descriptor == null
      ? null
      : typeof descriptor === "string"
      ? (() => { try { return JSON.parse(descriptor) as AuthorDescriptor; } catch { return null; } })()
      : descriptor;

  // Stable cache key — drives both instant resolution and the Tauri fetch
  const descriptorJson = JSON.stringify(desc ?? { type: "local" });

  const [profile, setProfile] = useState<AuthorProfile | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const lastFetched = useRef<string>("");

  useEffect(() => {
    if (lastFetched.current === descriptorJson) return;
    lastFetched.current = descriptorJson;

    // 1. Check module-level cache — already have a rich profile, use it immediately.
    const cached = profileCache.get(descriptorJson);
    if (cached) {
      setProfile(cached);
      setState("ready");
      return;
    }

    // 2. Client-side instant resolution — show something immediately, no skeleton.
    //    For github type this is a CDN-based fallback; for local/provider it's final.
    const instant = resolveLocally(desc);
    if (instant) {
      setProfile(instant);
      setState("ready");
    }

    // 3. For github type, attempt a Tauri upgrade in the background to get the
    //    real display name and bio. The CDN profile above is already visible, so
    //    any improvement is a nice-to-have rather than a blocker.
    if (desc?.type !== "github") return;

    let cancelled = false;

    // Race against a 5-second timeout — if Tauri is slow we already have a
    // perfectly usable CDN-based profile showing.
    const timeout = new Promise<AuthorProfile>((_, reject) =>
      setTimeout(() => reject(new Error("resolve_author timed out")), 5000)
    );

    Promise.race([
      invoke<AuthorProfile>("resolve_author", { descriptor: descriptorJson }),
      timeout,
    ])
      .then((p) => {
        if (!cancelled) {
          profileCache.set(descriptorJson, p);
          setProfile(p);
        }
      })
      .catch(() => {
        // Instant CDN profile is already showing — nothing to do on failure.
        if (!cancelled && instant) {
          profileCache.set(descriptorJson, instant);
        }
      });

    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [descriptorJson]);

  // ── Skeleton while loading ────────────────────────────────────────────────
  if (state === "loading" || !profile) {
    return (
      <div className="flex items-center gap-3 py-1">
        <div className="w-10 h-10 rounded-full bg-surface animate-pulse flex-shrink-0" />
        <div className="space-y-1.5 flex-1 min-w-0">
          <div className="h-3 w-24 rounded bg-surface animate-pulse" />
          <div className="h-2.5 w-40 rounded bg-surface animate-pulse" />
        </div>
      </div>
    );
  }

  const isLocal = profile.kind === "local";
  const hasLink = !!profile.url;

  const nameEl = (
    <span className="text-[13px] font-semibold text-text-base leading-snug">
      {profile.name}
    </span>
  );

  return (
    <div className="flex items-center gap-3 py-0.5 group">
      {/* Avatar */}
      {isLocal ? (
        <div
          className="w-10 h-10 rounded-full bg-surface border border-border-strong/40 flex items-center justify-center flex-shrink-0"
        >
          <HardDrive size={16} className="text-text-muted" />
        </div>
      ) : (
        <Avatar url={profile.avatar_url} name={profile.name} size={40} />
      )}

      {/* Name + bio */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 min-w-0">
          {hasLink ? (
            <a
              href={profile.url}
              target="_blank"
              rel="noopener noreferrer"
              className="hover:text-brand transition-colors flex items-center gap-1 min-w-0"
              onClick={(e) => e.stopPropagation()}
            >
              {nameEl}
              <ExternalLink
                size={10}
                className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0"
              />
            </a>
          ) : (
            nameEl
          )}
        </div>
        {profile.bio && (
          <p className="text-[11px] text-text-muted leading-snug truncate mt-0.5">
            {profile.bio}
          </p>
        )}
      </div>
    </div>
  );
}

// ── AuthorSection ─────────────────────────────────────────────────────────────

/**
 * Labelled "AUTHOR" section for detail panels.
 *
 * Usage:
 *   <AuthorSection descriptor={{ type: "local" }} />
 *   <AuthorSection descriptor={{ type: "github", repo: "vercel-labs/agent-skills" }} />
 *   <AuthorSection descriptor={item._author} />
 */
export function AuthorSection({ descriptor }: AuthorPanelProps) {
  return (
    <div>
      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase block mb-2">
        Author
      </span>
      <AuthorPanel descriptor={descriptor} />
    </div>
  );
}
