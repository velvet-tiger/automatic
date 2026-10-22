export type Theme = "system" | "dark" | "light" | "corporate-dark" | "corporate-light" | "accessible" | "cyberpunk" | "synthwave-glow" | "coral" | "arctic-light";

export const THEMES: { id: Theme; name: string; description: string; colors: { primary: string; surface: string } }[] = [
  {
    id: "dark",
    name: "Dark",
    description: "The default Automatic dark mode with subtle Indigo highlights.",
    colors: { primary: "#6366f1", surface: "#18181b" }
  },
  {
    id: "light",
    name: "Light",
    description: "The default Automatic light mode with crisp Indigo highlights.",
    colors: { primary: "#4f46e5", surface: "#ffffff" }
  },
  {
    id: "corporate-dark",
    name: "Corporate Dark",
    description: "Pure monochrome — black, dark grey, and white. No colour.",
    colors: { primary: "#2a2a2a", surface: "#1a1a1a" }
  },
  {
    id: "corporate-light",
    name: "Corporate Light",
    description: "Pure monochrome light — white, light grey, and near-black. No colour.",
    colors: { primary: "#2a2a2a", surface: "#ffffff" }
  },
  {
    id: "accessible",
    name: "Accessible",
    description: "WCAG AA+, CVD-safe. High contrast blue on white — readable for all vision types.",
    colors: { primary: "#1d4ed8", surface: "#ffffff" }
  },
  {
    id: "cyberpunk",
    name: "Cyberpunk",
    description: "Deep dark backgrounds with vibrant Cyan and Electric Purple.",
    colors: { primary: "#00f0ff", surface: "#18181b" }
  },
  {
    id: "synthwave-glow",
    name: "Synthwave Glow",
    description: "Rich dark purples with glowing Magenta and Amber.",
    colors: { primary: "#ec4899", surface: "#4c1d95" }
  },
  {
    id: "coral",
    name: "Coral",
    description: "Stark dark mode with vibrant Coral and minimal Slate accents.",
    colors: { primary: "#f97316", surface: "#27272a" }
  },
  {
    id: "arctic-light",
    name: "Arctic Light",
    description: "Clean, crisp light mode with cool Blue and Teal highlights.",
    colors: { primary: "#0ea5e9", surface: "#ffffff" }
  }
];

/**
 * Returns the resolved theme id when "system" is selected,
 * i.e. "dark" or "light" based on the OS preference.
 */
export function resolveSystemTheme(): "dark" | "light" {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(theme: string) {
  if (theme === "sleek-hacker") theme = "corporate-dark";
  if (theme === "sleek") theme = "corporate-dark";
  if (theme === "neon-cyberpunk") theme = "cyberpunk";
  if (theme === "minimalist-coral") theme = "coral";

  // "system" resolves to dark or light based on OS preference
  if (theme === "system") theme = resolveSystemTheme();

  if (theme === "dark") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", theme);
  }
}

const THEME_STORAGE_KEY = "automatic.theme";
const THEME_CHANGED_EVENT = "automatic:theme-changed";

const LEGACY_THEME_IDS: Record<string, Theme> = {
  "sleek-hacker": "corporate-dark",
  sleek: "corporate-dark",
  "neon-cyberpunk": "cyberpunk",
  "minimalist-coral": "coral",
};

/** The saved theme id, with renamed legacy ids mapped forward. Defaults to "system". */
export function getStoredTheme(): Theme {
  const saved = localStorage.getItem(THEME_STORAGE_KEY);
  if (!saved) return "system";
  return LEGACY_THEME_IDS[saved] ?? (saved as Theme);
}

/**
 * Save, apply and announce a theme. Every theme change goes through here so
 * components that show the current theme (sidebar switcher, Settings) stay in step.
 */
export function setStoredTheme(theme: Theme): void {
  localStorage.setItem(THEME_STORAGE_KEY, theme);
  applyTheme(theme);
  window.dispatchEvent(new CustomEvent<Theme>(THEME_CHANGED_EVENT, { detail: theme }));
}

/** Subscribe to theme changes made through `setStoredTheme`. Returns an unsubscribe function. */
export function onThemeChanged(listener: (theme: Theme) => void): () => void {
  const handler = (e: Event) => listener((e as CustomEvent<Theme>).detail);
  window.addEventListener(THEME_CHANGED_EVENT, handler);
  return () => window.removeEventListener(THEME_CHANGED_EVENT, handler);
}
