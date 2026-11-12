import { fillUrlTemplate, urlTemplateTokenLabel, urlTemplateTokens } from "../lib/urlTemplate";

/**
 * One text field per `<token>` in a server URL template, with a preview of
 * the URL the values produce.
 *
 * Used where a catalogue server is added and in the library editor, so the
 * user only ever types the part of the address that is theirs. The preview
 * shows the template's placeholder until the values are valid. The problem
 * with the first invalid value shows once the user types, or at once with
 * `showMissing`, which the editor uses to explain why a saved server is broken.
 */
export function UrlTemplateFields({
  template,
  values,
  onChange,
  readOnly = false,
  inputClassName,
  onSubmit,
  showMissing = false,
  autoFocus = false,
}: {
  template: string;
  values: Record<string, string>;
  onChange: (values: Record<string, string>) => void;
  readOnly?: boolean;
  inputClassName: string;
  /** Called when the user presses Enter in a field. */
  onSubmit?: () => void;
  /** Show the problem before the user has typed anything. */
  showMissing?: boolean;
  /** Focus the first field on mount. */
  autoFocus?: boolean;
}) {
  const tokens = urlTemplateTokens(template);
  const filled = fillUrlTemplate(template, values);
  const anyTyped = tokens.some((t) => (values[t] ?? "").trim() !== "");

  return (
    <div className="space-y-2">
      {tokens.map((token, i) => {
        const label = urlTemplateTokenLabel(token);
        return (
          <label key={token} className="block">
            <span className="block text-[12px] text-text-muted mb-1">{label}</span>
            <input
              type="text"
              value={values[token] ?? ""}
              onChange={(e) => !readOnly && onChange({ ...values, [token]: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") onSubmit?.();
              }}
              readOnly={readOnly}
              placeholder={label.toLowerCase()}
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              autoFocus={autoFocus && i === 0 && !readOnly}
              aria-label={label}
              className={`${inputClassName} ${readOnly ? "opacity-60 cursor-not-allowed" : ""}`}
            />
          </label>
        );
      })}
      <p className="text-[11px] text-text-muted font-mono break-all">
        {"url" in filled ? filled.url : template}
      </p>
      {"problem" in filled && (anyTyped || showMissing) && (
        <p className="text-[11px] text-danger">{filled.problem}</p>
      )}
    </div>
  );
}
