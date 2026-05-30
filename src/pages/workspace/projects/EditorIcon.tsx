// Icon components for the Projects workspace feature.
//
// Extracted verbatim from Projects.tsx as part of a behavior-preserving refactor.

import { useState } from "react";
import { FolderOpen } from "lucide-react";
import type { ProjectToolEntry } from "./types";

/**
 * Editor icon component.
 *
 * When `iconPath` is provided (a local filesystem path returned by the
 * `get_editor_icon` Tauri command) it renders the real app bundle icon as a
 * data URI returned by the `get_editor_icon` Tauri command.  Otherwise it
 * falls back to inline SVG approximations so the UI is never blank.
 *
 * SVG fallbacks sourced from the opencode project (MIT licence):
 * https://github.com/anomalyco/opencode/tree/dev/packages/ui/src/assets/icons/app
 */
export function EditorIcon({ id, iconPath }: { id: string; iconPath?: string }) {
  const cls = "w-[16px] h-[16px] flex-shrink-0 rounded-[3px]";

  // iconPath is a "data:image/png;base64,..." URI from the Rust backend
  if (iconPath) {
    return (
      <img
        src={iconPath}
        alt={id}
        className={cls}
        style={{ objectFit: "contain" }}
      />
    );
  }
  switch (id) {
    case "finder":
      // macOS Finder — blue face icon
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="finder-bg" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#6AC4F9"/>
              <stop offset="100%" stopColor="#2176D9"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" fill="url(#finder-bg)"/>
          {/* Happy face - left half */}
          <ellipse cx="21" cy="32" rx="16" ry="22" fill="#E8F4FF"/>
          {/* Smiling face - right half */}
          <ellipse cx="43" cy="32" rx="16" ry="22" fill="#FFFFFF"/>
          {/* Eyes */}
          <circle cx="17" cy="26" r="4" fill="#2176D9"/>
          <circle cx="47" cy="26" r="4" fill="#48AFEE"/>
          <circle cx="16" cy="25" r="1.5" fill="white"/>
          <circle cx="46" cy="25" r="1.5" fill="white"/>
          {/* Smile */}
          <path d="M28 40 Q32 46 36 40" stroke="#555" strokeWidth="2" fill="none" strokeLinecap="round"/>
          {/* Nose */}
          <ellipse cx="32" cy="35" rx="2" ry="1.5" fill="#DDD"/>
        </svg>
      );
    case "vscode":
      // Real VS Code icon from opencode (MIT)
      return (
        <svg className={cls} xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 100 100">
          <mask id="vscode-a" width="100" height="100" x="0" y="0" maskUnits="userSpaceOnUse" style={{maskType:"alpha"}}>
            <path fill="#fff" fillRule="evenodd" d="M70.912 99.317a6.223 6.223 0 0 0 4.96-.19l20.589-9.907A6.25 6.25 0 0 0 100 83.587V16.413a6.25 6.25 0 0 0-3.54-5.632L75.874.874a6.226 6.226 0 0 0-7.104 1.21L29.355 38.04 12.187 25.01a4.162 4.162 0 0 0-5.318.236l-5.506 5.009a4.168 4.168 0 0 0-.004 6.162L16.247 50 1.36 63.583a4.168 4.168 0 0 0 .004 6.162l5.506 5.01a4.162 4.162 0 0 0 5.318.236l17.168-13.032L68.77 97.917a6.217 6.217 0 0 0 2.143 1.4ZM75.015 27.3 45.11 50l29.906 22.701V27.3Z" clipRule="evenodd"/>
          </mask>
          <g mask="url(#vscode-a)">
            <path fill="#0065A9" d="M96.461 10.796 75.857.876a6.23 6.23 0 0 0-7.107 1.207l-67.451 61.5a4.167 4.167 0 0 0 .004 6.162l5.51 5.009a4.167 4.167 0 0 0 5.32.236l81.228-61.62c2.725-2.067 6.639-.124 6.639 3.297v-.24a6.25 6.25 0 0 0-3.539-5.63Z"/>
            <path fill="#007ACC" d="m96.461 89.204-20.604 9.92a6.229 6.229 0 0 1-7.107-1.207l-67.451-61.5a4.167 4.167 0 0 1 .004-6.162l5.51-5.009a4.167 4.167 0 0 1 5.32-.236l81.228 61.62c2.725 2.067 6.639.124 6.639-3.297v.24a6.25 6.25 0 0 1-3.539 5.63Z"/>
            <path fill="#1F9CF0" d="M75.858 99.126a6.232 6.232 0 0 1-7.108-1.21c2.306 2.307 6.25.674 6.25-2.588V4.672c0-3.262-3.944-4.895-6.25-2.589a6.232 6.232 0 0 1 7.108-1.21l20.6 9.908A6.25 6.25 0 0 1 100 16.413v67.174a6.25 6.25 0 0 1-3.541 5.633l-20.601 9.906Z"/>
          </g>
        </svg>
      );
    case "cursor":
      // Real Cursor icon from opencode (MIT)
      return (
        <svg className={cls} fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
          <rect width="512" height="512" rx="122" fill="#000"/>
          <g clipPath="url(#cursor-clip)">
            <mask id="cursor-mask" style={{maskType:"luminance"}} maskUnits="userSpaceOnUse" x="85" y="89" width="343" height="334">
              <path d="M85 89h343v334H85V89z" fill="#fff"/>
            </mask>
            <g mask="url(#cursor-mask)">
              <path d="M255.428 423l148.991-83.5L255.428 256l-148.99 83.5 148.99 83.5z" fill="url(#cursor-g0)"/>
              <path d="M404.419 339.5v-167L255.428 89v167l148.991 83.5z" fill="url(#cursor-g1)"/>
              <path d="M255.428 89l-148.99 83.5v167l148.99-83.5V89z" fill="url(#cursor-g2)"/>
              <path d="M404.419 172.5L255.428 423V256l148.991-83.5z" fill="#E4E4E4"/>
              <path d="M404.419 172.5L255.428 256l-148.99-83.5h297.981z" fill="#fff"/>
            </g>
          </g>
          <defs>
            <linearGradient id="cursor-g0" x1="255.428" y1="256" x2="255.428" y2="423" gradientUnits="userSpaceOnUse">
              <stop offset=".16" stopColor="#fff" stopOpacity=".39"/>
              <stop offset=".658" stopColor="#fff" stopOpacity=".8"/>
            </linearGradient>
            <linearGradient id="cursor-g1" x1="404.419" y1="173.015" x2="257.482" y2="261.497" gradientUnits="userSpaceOnUse">
              <stop offset=".182" stopColor="#fff" stopOpacity=".31"/>
              <stop offset=".715" stopColor="#fff" stopOpacity="0"/>
            </linearGradient>
            <linearGradient id="cursor-g2" x1="255.428" y1="89" x2="112.292" y2="342.802" gradientUnits="userSpaceOnUse">
              <stop stopColor="#fff" stopOpacity=".6"/>
              <stop offset=".667" stopColor="#fff" stopOpacity=".22"/>
            </linearGradient>
            <clipPath id="cursor-clip">
              <path fill="#fff" transform="translate(85 89)" d="M0 0h343v334H0z"/>
            </clipPath>
          </defs>
        </svg>
      );
    case "zed":
      // Real Zed icon from opencode (MIT) — white on dark background
      return (
        <svg className={cls} xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" fill="none">
          <rect width="96" height="96" rx="18" fill="#084CCE"/>
          <g clipPath="url(#zed-clip)">
            <path fill="#fff" fillRule="evenodd" d="M9 6a3 3 0 0 0-3 3v66H0V9a9 9 0 0 1 9-9h80.379c4.009 0 6.016 4.847 3.182 7.682L43.055 57.187H57V51h6v7.688a4.5 4.5 0 0 1-4.5 4.5H37.055L26.743 73.5H73.5V36h6v37.5a6 6 0 0 1-6 6H20.743L10.243 90H87a3 3 0 0 0 3-3V21h6v66a9 9 0 0 1-9 9H6.621c-4.009 0-6.016-4.847-3.182-7.682L52.757 39H39v6h-6v-7.5a4.5 4.5 0 0 1 4.5-4.5h21.257l10.5-10.5H22.5V60h-6V22.5a6 6 0 0 1 6-6h52.757L85.757 6H9Z" clipRule="evenodd"/>
          </g>
          <defs>
            <clipPath id="zed-clip"><path fill="#fff" d="M0 0h96v96H0z"/></clipPath>
          </defs>
        </svg>
      );
    case "textmate":
      // TextMate — ball-in-circle logo style, golden/dark
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect width="64" height="64" rx="14" fill="#1C1C1C"/>
          <circle cx="32" cy="32" r="20" fill="#2A2A2A" stroke="#4A4A4A" strokeWidth="1.5"/>
          <circle cx="32" cy="32" r="13" fill="#E8A820"/>
          <circle cx="32" cy="32" r="8" fill="#C88A10"/>
          <circle cx="28" cy="28" r="3" fill="#FFCF50"/>
          <text x="32" y="56" textAnchor="middle" fontSize="10" fontWeight="700" fill="#888" fontFamily="monospace">TM</text>
        </svg>
      );
    case "antigravity":
      // Real Antigravity icon from opencode (MIT)
      return (
        <svg className={cls} viewBox="0 0 16 15" fill="none" xmlns="http://www.w3.org/2000/svg">
          <mask id="ag-mask" style={{maskType:"alpha"}} maskUnits="userSpaceOnUse" x="0" y="0" width="16" height="15">
            <path d="M14.0777 13.984C14.945 14.6345 16.2458 14.2008 15.0533 13.0084C11.476 9.53949 12.2349 0 7.79033 0C3.34579 0 4.10461 9.53949 0.527295 13.0084C-0.773543 14.3092 0.635692 14.6345 1.50293 13.984C4.86344 11.7076 4.64663 7.69664 7.79033 7.69664C10.934 7.69664 10.7172 11.7076 14.0777 13.984Z" fill="black"/>
          </mask>
          <g mask="url(#ag-mask)">
            <g filter="url(#ag-f0)"><path d="-0.658907 -3.2306C-0.922679 -0.906781 1.07986 1.22861 3.81388 1.53894C6.54791 1.84927 8.97811 0.217009 9.24188 -2.10681C9.50565 -4.43063 7.50312 -6.56602 4.76909 -6.87635C2.03506 -7.18667 -0.395135 -5.55442 -0.658907 -3.2306Z" fill="#FFE432"/></g>
            <g filter="url(#ag-f1)"><path d="M9.88233 4.36642C10.5673 7.31568 13.566 9.13902 16.5801 8.43896C19.5942 7.73891 21.4823 4.78056 20.7973 1.83131C20.1123 -1.11795 17.1136 -2.94128 14.0995 -2.24123C11.0854 -1.54118 9.19733 1.41717 9.88233 4.36642Z" fill="#FC413D"/></g>
            <g filter="url(#ag-f2)"><path d="M-8.05291 6.34512C-7.18736 9.38883 -3.28925 10.9473 0.653774 9.82598C4.5968 8.7047 7.09158 5.32829 6.22603 2.28458C5.36048 -0.759142 1.46236 -2.31758 -2.48066 -1.19629C-6.42368 -0.0750048 -8.91846 3.3014 -8.05291 6.34512Z" fill="#00B95C"/></g>
            <g filter="url(#ag-f3)"><path d="M6.42819 17.2263C7.10197 20.1273 9.91278 21.953 12.7063 21.3042C15.4998 20.6553 17.2182 17.7777 16.5444 14.8767C15.8707 11.9757 13.0599 10.15 10.2663 10.7988C7.47281 11.4477 5.75441 14.3253 6.42819 17.2263Z" fill="#3186FF"/></g>
          </g>
          <defs>
            <filter id="ag-f0" x="-2.13" y="-8.36" width="12.84" height="11.38" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="0.72" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f1" x="2.75" y="-9.38" width="25.18" height="24.96" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="3.5" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f2" x="-14.17" y="-7.5" width="26.51" height="23.63" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="2.97" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f3" x="0.63" y="5.02" width="21.7" height="22.06" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="2.82" result="effect1_foregroundBlur"/></filter>
          </defs>
        </svg>
      );
    case "xcode":
      // Xcode — hammer + wrench on blue gradient background
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="xcode-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#3A8EFF"/>
              <stop offset="100%" stopColor="#0F5FD8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#xcode-bg)"/>
          {/* Hammer handle */}
          <rect x="38" y="36" width="6" height="18" rx="2" transform="rotate(-45 38 36)" fill="#E8E8E8"/>
          {/* Hammer head */}
          <rect x="22" y="12" width="18" height="12" rx="3" fill="white"/>
          {/* Wrench */}
          <circle cx="42" cy="20" r="8" fill="none" stroke="#B0C8FF" strokeWidth="4"/>
          <rect x="40" y="24" width="4" height="18" rx="2" fill="#B0C8FF"/>
        </svg>
      );
    // ── JetBrains IDEs ─────────────────────────────────────────────
    // All use the JetBrains icon pattern: gradient bg + black inset with abbreviation
    case "intellij":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ij-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#087CFA"/>
              <stop offset="100%" stopColor="#FE315D"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ij-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">IJ</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "phpstorm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ps-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#B345F1"/>
              <stop offset="100%" stopColor="#765AF8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ps-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">PS</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "webstorm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ws-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#07C3F2"/>
              <stop offset="100%" stopColor="#087CFA"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ws-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="12" y="32" fontSize="14" fontWeight="700" fill="#fff" fontFamily="sans-serif">WS</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "pycharm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="pc-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#21D789"/>
              <stop offset="100%" stopColor="#FCF84A"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#pc-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">PC</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "rustrover":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="rr-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#F26522"/>
              <stop offset="100%" stopColor="#FDB811"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#rr-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">RR</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "clion":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="cl-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#21D789"/>
              <stop offset="100%" stopColor="#009AE5"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#cl-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">CL</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "goland":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="gl-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#087CFA"/>
              <stop offset="100%" stopColor="#765AF8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#gl-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">GL</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "datagrip":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="dg-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#22D88F"/>
              <stop offset="100%" stopColor="#9775F8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#dg-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="12" y="32" fontSize="14" fontWeight="700" fill="#fff" fontFamily="sans-serif">DG</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "rider":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="rd-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#C90F5E"/>
              <stop offset="100%" stopColor="#087CFA"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#rd-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">RD</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    default:
      return <FolderOpen size={14} className="text-text-muted" />;
  }
}

export function ProjectToolAvatar({ tool, size = 28 }: { tool: ProjectToolEntry; size?: number }) {
  const [broken, setBroken] = useState(false);
  const owner = tool.github_repo ? tool.github_repo.split("/")[0] : null;
  const avatarUrl = owner ? `https://github.com/${owner}.png?size=${size * 2}` : null;
  const letter = tool.display_name.charAt(0).toUpperCase();

  if (avatarUrl && !broken) {
    return (
      <img
        src={avatarUrl}
        alt={owner ?? tool.display_name}
        width={size}
        height={size}
        className="rounded-md object-cover flex-shrink-0"
        style={{ width: size, height: size }}
        onError={() => setBroken(true)}
      />
    );
  }
  return (
    <div
      className="rounded-md flex items-center justify-center font-semibold bg-icon-skill/15 text-icon-skill flex-shrink-0"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.44) }}
      aria-hidden="true"
    >
      {letter}
    </div>
  );
}
