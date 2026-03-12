import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FileText, Hash, DollarSign, AlertCircle, FolderOpen, X, Info } from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

interface TokenEstimate {
  model: string;
  provider: string;
  tokens: number;
  cost_per_million_input: number;
  cost_per_million_output: number;
  estimated_input_cost: number;
  method: "exact" | "approximate";
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function formatCost(usd: number): string {
  if (usd === 0) return "$0.00";
  if (usd < 0.000001) return "< $0.000001";
  if (usd < 0.01) return `$${usd.toFixed(6)}`;
  return `$${usd.toFixed(4)}`;
}

const PROVIDER_COLORS: Record<string, string> = {
  Anthropic: "text-[#d97706]",
  OpenAI: "text-[#10a37f]",
  Google: "text-[#4285f4]",
};

const PROVIDER_BG: Record<string, string> = {
  Anthropic: "bg-[#d97706]/10 border-[#d97706]/20",
  OpenAI: "bg-[#10a37f]/10 border-[#10a37f]/20",
  Google: "bg-[#4285f4]/10 border-[#4285f4]/20",
};

// ── Sub-components ────────────────────────────────────────────────────────────

function EstimateRow({ estimate }: { estimate: TokenEstimate }) {
  const color = PROVIDER_COLORS[estimate.provider] ?? "text-text-muted";
  const bg = PROVIDER_BG[estimate.provider] ?? "bg-bg-sidebar border-border-strong/20";

  return (
    <div className={`rounded-md border px-4 py-3 ${bg}`}>
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={`text-[11px] font-semibold uppercase tracking-wider ${color}`}>
              {estimate.provider}
            </span>
            {estimate.method === "approximate" && (
              <span
                className="text-[10px] text-text-muted bg-bg-sidebar border border-border-strong/30 rounded px-1.5 py-0.5"
                title="Token count is an approximation based on character ratios. No public tokeniser is available for this provider."
              >
                ~approx
              </span>
            )}
          </div>
          <div className="text-[13px] font-medium text-text-base mt-0.5 truncate">
            {estimate.model}
          </div>
        </div>

        <div className="flex items-center gap-6 shrink-0">
          {/* Token count */}
          <div className="text-right">
            <div className="text-[11px] text-text-muted mb-0.5">Tokens</div>
            <div className="text-[14px] font-semibold text-text-base font-mono">
              {formatTokens(estimate.tokens)}
            </div>
          </div>

          {/* Input cost */}
          <div className="text-right min-w-[80px]">
            <div className="text-[11px] text-text-muted mb-0.5">Input cost</div>
            <div className="text-[14px] font-semibold text-text-base font-mono">
              {formatCost(estimate.estimated_input_cost)}
            </div>
          </div>

          {/* Rate */}
          <div className="text-right min-w-[90px] hidden sm:block">
            <div className="text-[11px] text-text-muted mb-0.5">Per 1M tokens</div>
            <div className="text-[12px] text-text-muted font-mono">
              ${estimate.cost_per_million_input.toFixed(2)} in
            </div>
            <div className="text-[12px] text-text-muted font-mono">
              ${estimate.cost_per_million_output.toFixed(2)} out
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

export default function TokenEstimator() {
  const [text, setText] = useState("");
  const [filePath, setFilePath] = useState<string | null>(null);
  const [estimates, setEstimates] = useState<TokenEstimate[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const charCount = text.length;
  const wordCount = text.trim() === "" ? 0 : text.trim().split(/\s+/).length;

  const handleEstimate = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const result = await invoke<TokenEstimate[]>("estimate_tokens", {
        request: filePath
          ? { file_path: filePath, text: null }
          : { text: text || null, file_path: null },
      });
      setEstimates(result);
    } catch (e) {
      setError(String(e));
      setEstimates(null);
    } finally {
      setLoading(false);
    }
  }, [text, filePath]);

  const handleOpenFile = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Text files", extensions: ["txt", "md", "ts", "tsx", "js", "jsx", "py", "rs", "go", "java", "cs", "cpp", "c", "h", "json", "yaml", "yml", "toml", "html", "css"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (selected && typeof selected === "string") {
        setFilePath(selected);
        setText("");
        setEstimates(null);
        setError(null);
      }
    } catch (e) {
      setError(`Failed to open file: ${e}`);
    }
  }, []);

  const handleClearFile = useCallback(() => {
    setFilePath(null);
    setEstimates(null);
    setError(null);
  }, []);

  const handleTextChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value);
    setFilePath(null);
    setEstimates(null);
    setError(null);
  }, []);

  const canEstimate = !loading && (filePath !== null || text.trim().length > 0);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6 space-y-5 custom-scrollbar">

        {/* Info banner */}
        <div className="flex items-start gap-3 px-4 py-3 rounded-md bg-bg-sidebar border border-border-strong/30 text-[12px] text-text-muted">
          <Info size={14} className="shrink-0 mt-0.5 text-text-muted" />
          <span>
            OpenAI models use <strong className="text-text-base">exact</strong> BPE counting via tiktoken.
            Claude and Gemini use <strong className="text-text-base">character-ratio approximations</strong> (~5–10% variance) since no public tokenisers exist for those providers.
          </span>
        </div>

        {/* Input section */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-[13px] font-semibold text-text-base">Input</h2>
            <div className="flex items-center gap-2">
              {/* Stats */}
              {!filePath && text.length > 0 && (
                <span className="text-[11px] text-text-muted">
                  {charCount.toLocaleString()} chars · {wordCount.toLocaleString()} words
                </span>
              )}
              {/* File picker */}
              <button
                onClick={handleOpenFile}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[12px] font-medium text-text-muted bg-bg-sidebar border border-border-strong/30 hover:border-border-strong/60 hover:text-text-base transition-colors"
              >
                <FolderOpen size={12} />
                Open file
              </button>
            </div>
          </div>

          {/* File pill */}
          {filePath && (
            <div className="flex items-center gap-2 px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/30">
              <FileText size={13} className="text-text-muted shrink-0" />
              <span className="text-[12px] text-text-base flex-1 truncate font-mono">{filePath}</span>
              <button
                onClick={handleClearFile}
                className="text-text-muted hover:text-text-base transition-colors"
                title="Remove file"
              >
                <X size={13} />
              </button>
            </div>
          )}

          {/* Textarea */}
          {!filePath && (
            <textarea
              className="w-full h-48 px-4 py-3 rounded-md bg-bg-input border border-border-strong/40 text-[13px] text-text-base placeholder:text-text-muted focus:outline-none focus:border-brand/50 resize-none font-mono custom-scrollbar"
              placeholder="Paste text here to estimate token counts…"
              value={text}
              onChange={handleTextChange}
              spellCheck={false}
            />
          )}
        </div>

        {/* Estimate button */}
        <button
          onClick={handleEstimate}
          disabled={!canEstimate}
          className="flex items-center gap-2 px-4 py-2 rounded-md text-[13px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Hash size={14} />
          {loading ? "Estimating…" : "Estimate tokens"}
        </button>

        {/* Error */}
        {error && (
          <div className="flex items-start gap-2 px-4 py-3 rounded-md bg-red-500/10 border border-red-500/20 text-[12px] text-red-400">
            <AlertCircle size={14} className="shrink-0 mt-0.5" />
            {error}
          </div>
        )}

        {/* Results */}
        {estimates && estimates.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <DollarSign size={14} className="text-text-muted" />
              <h2 className="text-[13px] font-semibold text-text-base">Estimates</h2>
            </div>

            {/* Group by provider */}
            {["Anthropic", "OpenAI", "Google"].map((provider) => {
              const group = estimates.filter((e) => e.provider === provider);
              if (group.length === 0) return null;
              return (
                <div key={provider} className="space-y-2">
                  <div className={`text-[11px] font-semibold tracking-wider uppercase ${PROVIDER_COLORS[provider] ?? "text-text-muted"}`}>
                    {provider}
                  </div>
                  {group.map((estimate) => (
                    <EstimateRow key={`${estimate.provider}-${estimate.model}`} estimate={estimate} />
                  ))}
                </div>
              );
            })}

            {/* Prices footer */}
            <p className="text-[11px] text-text-muted pt-1">
              Prices are indicative as of early 2025 and may have changed. Always verify with each provider's pricing page.
            </p>
          </div>
        )}

        {estimates && estimates.length === 0 && (
          <div className="text-[13px] text-text-muted text-center py-6">
            No content to estimate — the input was empty.
          </div>
        )}
      </div>
    </div>
  );
}
