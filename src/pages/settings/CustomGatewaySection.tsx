import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight, Eye, EyeOff } from "lucide-react";

export type GatewayCredentialKind = "bearer" | "api_key";

/** What the backend reports about the gateway. It never includes the token. */
export interface CustomGatewayStatus {
  configured: boolean;
  base_url: string | null;
  credential_kind: GatewayCredentialKind | null;
  /** The record is gone but a token is stored. Claude requests are refused. */
  orphaned_token: boolean;
}

interface ClearedCustomGateway {
  left_in_claude_code: string[];
}

interface CustomGatewaySectionProps {
  status: CustomGatewayStatus;
  /** True when a Cloudflare gateway is saved and this section overrides it. */
  overridesCloudflare: boolean;
  /** Called after a save or clear so the page can reload what depends on it. */
  onChanged: () => Promise<void> | void;
}

/**
 * Whether two URLs share a scheme, host and port. The backend only keeps a
 * saved token for the same address, and it makes the final decision.
 */
function sameAddress(first: string, second: string): boolean {
  try {
    return new URL(first.trim()).origin === new URL(second.trim()).origin;
  } catch {
    return false;
  }
}

/** A sentence naming the gateway variables that stayed in Claude Code's file. */
export function leftInPlaceNotice(names: string[]): string {
  const list = names.join(", ");
  if (names.length === 1) {
    return `${list} is still set in ~/.claude/settings.json. Automatic left it in place. Remove it from that file if you no longer want it.`;
  }
  return `${list} are still set in ~/.claude/settings.json. Automatic left them in place. Remove them from that file if you no longer want them.`;
}

const inputClass =
  "w-full text-[12px] text-text-base bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand transition-colors";

/**
 * The custom LLM gateway form on the Claude card. The backend validates the
 * values and returns the error text shown here.
 */
export function CustomGatewaySection({ status, overridesCloudflare, onChanged }: CustomGatewaySectionProps) {
  const [expanded, setExpanded] = useState(false);
  const [baseUrl, setBaseUrl] = useState("");
  const [kind, setKind] = useState<GatewayCredentialKind>("bearer");
  const [token, setToken] = useState("");
  const [revealed, setRevealed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    setBaseUrl(status.base_url ?? "");
    setKind(status.credential_kind ?? "bearer");
  }, [status.base_url, status.credential_kind]);

  const tokenKept = status.base_url !== null && sameAddress(baseUrl, status.base_url);
  const canSave = !busy && baseUrl.trim() !== "" && (tokenKept || token.trim() !== "");
  const tokenPlaceholder = tokenKept
    ? "Leave blank to keep the current token"
    : status.configured
      ? "Enter the token for this address"
      : "Token";

  const closeForm = () => {
    setExpanded(false);
    setToken("");
    setRevealed(false);
    setError(null);
    setBaseUrl(status.base_url ?? "");
    setKind(status.credential_kind ?? "bearer");
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("save_custom_gateway", {
        baseUrl,
        credentialKind: kind,
        token: token.trim() === "" ? null : token,
      });
      setToken("");
      setRevealed(false);
      setExpanded(false);
      await onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const clear = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const cleared = await invoke<ClearedCustomGateway>("clear_custom_gateway");
      setToken("");
      setRevealed(false);
      setExpanded(false);
      const leftInPlace = cleared?.left_in_claude_code ?? [];
      if (leftInPlace.length > 0) {
        setNotice(`Gateway cleared. ${leftInPlaceNotice(leftInPlace)}`);
      }
      await onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="border-t border-border-strong/20">
      <button
        onClick={() => (expanded ? closeForm() : setExpanded(true))}
        className="flex items-center gap-2 w-full px-3 py-2.5 text-left hover:bg-surface-hover transition-colors"
      >
        {expanded
          ? <ChevronDown size={11} className="text-text-muted flex-shrink-0" />
          : <ChevronRight size={11} className="text-text-muted flex-shrink-0" />
        }
        <span className="text-[11px] text-text-muted flex-1">Custom gateway</span>
        {status.orphaned_token ? (
          <span className="text-[11px] text-danger font-medium">Needs attention</span>
        ) : status.configured ? (
          <span className="text-[11px] text-success font-medium">Active</span>
        ) : (
          <span className="text-[11px] text-text-muted">Off</span>
        )}
      </button>

      {status.orphaned_token && (
        <p className="px-3 pb-3 text-[11px] text-danger leading-relaxed">
          Automatic has lost its gateway record, but the token is still stored. Claude requests
          from Automatic are blocked, and Claude Code may still use the gateway. Open this section
          to save the gateway again or clear it.
        </p>
      )}
      {notice && !expanded && (
        <p className="px-3 pb-3 text-[11px] text-text-muted leading-relaxed">{notice}</p>
      )}
      {error && !expanded && (
        <p className="px-3 pb-3 text-[11px] text-danger leading-relaxed">{error}</p>
      )}

      {expanded && (
        <div className="px-3 pb-3 space-y-2">
          <p className="text-[11px] text-text-muted leading-relaxed">
            Sends Claude requests through your own gateway, both from Automatic and from Claude Code.
          </p>
          <input
            type="text"
            aria-label="Gateway URL"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://gateway.example.com"
            className={inputClass}
          />
          <div className="relative">
            <select
              aria-label="Credential type"
              value={kind}
              onChange={(e) => setKind(e.target.value as GatewayCredentialKind)}
              className={`${inputClass} appearance-none pr-7`}
            >
              <option value="bearer">Bearer token</option>
              <option value="api_key">API key</option>
            </select>
            <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
          </div>
          <div className="relative">
            <input
              type={revealed ? "text" : "password"}
              aria-label="Gateway token"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={tokenPlaceholder}
              className={`${inputClass} font-mono pr-9`}
            />
            <button
              onClick={() => setRevealed(!revealed)}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
              title={revealed ? "Hide" : "Reveal"}
              type="button"
            >
              {revealed ? <EyeOff size={13} /> : <Eye size={13} />}
            </button>
          </div>

          <ul className="text-[11px] text-text-muted leading-relaxed list-disc pl-4 space-y-0.5">
            <li>
              Automatic keeps the token in the OS keychain. It also writes the token in plain text
              to ~/.claude/settings.json, because Claude Code reads it there.
            </li>
            <li>Claude Code will use this gateway instead of your claude.ai login.</li>
            <li>Remote Control and voice dictation stop working until you clear the gateway.</li>
            <li>The Claude desktop app has its own gateway settings and does not use this one.</li>
            <li>
              With the API key type, interactive Claude Code asks you once to approve the key. If
              you decline, it ignores the key from then on.
            </li>
            <li>The VS Code extension's login check does not read ~/.claude/settings.json.</li>
            <li>
              A ~/.claude/settings.json linked to a file outside ~/.claude cannot be used, because
              the token could end up in a tracked repository.
            </li>
            {overridesCloudflare && (
              <li>While this gateway is on, Automatic does not use your Cloudflare AI Gateway.</li>
            )}
          </ul>

          <div className="flex items-center gap-2 pt-1">
            <button
              onClick={save}
              disabled={!canSave}
              className="px-3 py-1.5 text-[11px] font-medium rounded bg-brand text-white hover:bg-brand-active disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {busy ? "Saving..." : "Save"}
            </button>
            {(status.configured || status.orphaned_token) && (
              <button
                onClick={clear}
                disabled={busy}
                className="px-3 py-1.5 text-[11px] font-medium rounded border border-danger/50 text-danger hover:border-danger hover:bg-danger/10 transition-all"
              >
                Clear
              </button>
            )}
            <button
              onClick={closeForm}
              className="px-2 py-1.5 text-[11px] text-text-muted hover:text-text-base transition-colors"
            >
              Cancel
            </button>
          </div>
          {error && <p className="text-[11px] text-danger leading-relaxed">{error}</p>}
        </div>
      )}
    </div>
  );
}
