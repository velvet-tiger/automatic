import React, { useState, useEffect } from "react";
import { Code } from "lucide-react";
import graphLogo from "../../logos/graph_5.svg";

interface SkillAvatarProps {
  /**
   * The skill name — used to derive the first-letter fallback.
   */
  name: string;
  /**
   * GitHub source slug in "owner/repo" format, if the skill was imported from
   * a remote repository.  When present, we attempt to load the GitHub owner's
   * avatar (`https://github.com/<owner>.png`).  On load failure we fall back
   * to the letter avatar.
   */
  source?: string;
  /**
   * Source kind — "github" fetches an owner avatar; "bundled" (or any other
   * value) skips the network fetch and goes straight to the letter fallback.
   * When omitted and `source` is present, we assume "github" for backwards
   * compatibility.
   */
  kind?: string;
  /**
   * Side length in pixels.  Defaults to 32 (matching the existing sidebar icon
   * box size).
   */
  size?: number;
  /** Extra class names applied to the outer container. */
  className?: string;
}

/**
 * Displays a skill's avatar.
 *
 * Priority:
 *  1. Automatic app icon for bundled skills (`kind === "bundled"`)
 *  2. GitHub owner avatar derived from `source` ("owner/repo"), only when
 *     `kind` is "github" (or unset with a source present).
 *  3. First-letter avatar (tinted with the skill icon colour)
 *  4. Generic <Code> lucide icon (only if name is somehow empty)
 */
export function SkillAvatar({ name, source, kind, size = 32, className = "" }: SkillAvatarProps) {
  const isBundled = kind === "bundled";

  // Only attempt a GitHub avatar fetch for genuine GitHub-hosted skills.
  // Bundled skills use the Automatic app icon instead.
  const isGitHub = source && !isBundled;
  const owner = isGitHub ? source.split("/")[0] : null;
  const avatarUrl = owner ? `https://github.com/${owner}.png?size=${size * 2}` : null;

  const [broken, setBroken] = useState(false);

  // Reset broken state whenever the URL changes (different skill selected).
  useEffect(() => {
    setBroken(false);
  }, [avatarUrl]);

  const letter = name ? name.charAt(0).toUpperCase() : null;

  const containerStyle: React.CSSProperties = {
    width: size,
    height: size,
    flexShrink: 0,
  };

  // ── Automatic logo (bundled skills) ─────────────────────────────────────────
  if (isBundled) {
    return (
      <div
        className={`rounded-md flex items-center justify-center bg-icon-skill/15 flex-shrink-0 ${className}`}
        style={containerStyle}
      >
        <img
          src={graphLogo}
          alt="Automatic"
          width={Math.round(size * 0.55)}
          height={Math.round(size * 0.55)}
        />
      </div>
    );
  }

  // ── GitHub avatar ──────────────────────────────────────────────────────────
  if (avatarUrl && !broken) {
    return (
      <img
        src={avatarUrl}
        alt={owner ?? name}
        width={size}
        height={size}
        className={`rounded-md object-cover flex-shrink-0 ${className}`}
        style={containerStyle}
        onError={() => setBroken(true)}
      />
    );
  }

  // ── Letter fallback ────────────────────────────────────────────────────────
  if (letter) {
    return (
      <div
        className={`rounded-md flex items-center justify-center font-semibold bg-icon-skill/15 text-icon-skill flex-shrink-0 ${className}`}
        style={{ ...containerStyle, fontSize: Math.round(size * 0.44) }}
        aria-hidden="true"
      >
        {letter}
      </div>
    );
  }

  // ── Generic icon (last resort) ─────────────────────────────────────────────
  return (
    <div
      className={`rounded-md flex items-center justify-center bg-icon-skill/12 flex-shrink-0 ${className}`}
      style={containerStyle}
      aria-hidden="true"
    >
      <Code size={Math.round(size * 0.47)} className="text-icon-skill" />
    </div>
  );
}
