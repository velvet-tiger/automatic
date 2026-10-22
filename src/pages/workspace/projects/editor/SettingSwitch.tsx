interface SettingSwitchProps {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onToggle: () => void;
}

/** The Enabled/Disabled switch used by the project Settings tab and the new-project wizard. */
export function SettingSwitch({ checked, disabled = false, label, onToggle }: SettingSwitchProps) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={onToggle}
      className={`flex items-center gap-2 px-2 py-1.5 rounded transition-colors flex-shrink-0 disabled:opacity-50 disabled:cursor-not-allowed ${
        checked ? "bg-brand/10 text-brand" : "bg-bg-sidebar text-text-muted"
      }`}
    >
      <span className="text-[11px] font-medium">{checked ? "Enabled" : "Disabled"}</span>
      <span
        className={`relative inline-flex h-4 w-7 items-center rounded-full transition-colors flex-shrink-0 ${
          checked ? "bg-brand" : "bg-border-strong/60"
        }`}
      >
        <span
          className={`inline-block h-3 w-3 transform rounded-full bg-white shadow transition-transform ${
            checked ? "translate-x-3.5" : "translate-x-0.5"
          }`}
        />
      </span>
    </button>
  );
}
