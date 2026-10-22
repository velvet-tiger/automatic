import { useEffect, useState } from "react";
import { ChevronDown } from "lucide-react";
import { getStoredTheme, onThemeChanged, setStoredTheme, Theme, THEMES } from "../lib/theme";

/** "System" follows the OS and is not in THEMES, so it gets its own entry. */
const SYSTEM_OPTION = {
  id: "system" as Theme,
  name: "System",
  colors: { primary: "#71717a" },
};

const OPTIONS: { id: Theme; name: string; colors: { primary: string } }[] = [SYSTEM_OPTION, ...THEMES];

export function SidebarThemeSwitcher() {
  const [open, setOpen] = useState(false);
  const [activeTheme, setActiveTheme] = useState<Theme>(getStoredTheme);

  useEffect(() => onThemeChanged(setActiveTheme), []);

  const current = OPTIONS.find((t) => t.id === activeTheme) ?? SYSTEM_OPTION;

  return (
    <div className="px-3 pt-3 pb-1 relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
      >
        <span
          className="w-2.5 h-2.5 rounded-full shrink-0"
          style={{ backgroundColor: current.colors.primary }}
        />
        <span className="flex-1 text-left">{current.name}</span>
        <ChevronDown size={12} className={`transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute bottom-full left-3 right-3 mb-1 rounded-md overflow-hidden bg-bg-input border border-border-strong/40">
          {OPTIONS.map((t) => (
            <button
              key={t.id}
              onClick={() => {
                setStoredTheme(t.id);
                setOpen(false);
              }}
              className={`w-full flex items-center gap-2.5 px-3 py-1.5 text-[13px] transition-colors ${
                t.id === activeTheme
                  ? "text-text-base bg-bg-sidebar"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
            >
              <span
                className="w-2.5 h-2.5 rounded-full shrink-0"
                style={{ backgroundColor: t.colors.primary }}
              />
              {t.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
