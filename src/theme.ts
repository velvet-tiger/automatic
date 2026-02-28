export type Theme = "dark" | "light" | "sleek" | "cyberpunk" | "synthwave-glow" | "coral" | "arctic-light" | "corporate-light";

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
    id: "sleek",
    name: "Sleek",
    description: "Muted dark surfaces with Electric Blue and Emerald Green highlights.",
    colors: { primary: "#3b82f6", surface: "#1e293b" }
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
  },
  {
    id: "corporate-light",
    name: "Corporate Light",
    description: "Professional light mode with subtle Indigo and deep Slate accents.",
    colors: { primary: "#4f46e5", surface: "#f8fafc" }
  }
];

export function applyTheme(theme: string) {
  if (theme === "sleek-hacker") theme = "sleek";
  if (theme === "neon-cyberpunk") theme = "cyberpunk";
  if (theme === "minimalist-coral") theme = "coral";

  if (theme === "dark") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", theme);
  }
}
