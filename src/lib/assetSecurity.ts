import { invoke } from "@tauri-apps/api/core";

export type AssetScanKind =
  | "skill"
  | "skill_manifest"
  | "companion_file"
  | "user_command"
  | "user_agent"
  | "rule"
  | "template";

export type AssetSecurityFindingSeverity = "warning" | "error";

export interface AssetSecurityFinding {
  severity: AssetSecurityFindingSeverity;
  code: string;
  message: string;
}

export interface AssetSecurityScanResult {
  blocked: boolean;
  findings: AssetSecurityFinding[];
}

export interface AssetSecurityScanRecord extends AssetSecurityScanResult {
  scanned_at: string;
}

interface AssetSecurityPresentationOptions {
  blockedLabel?: string;
  blockedHeader?: string;
}

export async function scanAssetContent(
  kind: AssetScanKind,
  content: string,
): Promise<AssetSecurityScanResult> {
  return invoke<AssetSecurityScanResult>("scan_asset_content", { kind, content });
}

export async function getSkillScanState(
  name: string,
): Promise<AssetSecurityScanRecord | null> {
  return invoke<AssetSecurityScanRecord | null>("get_skill_scan_state", { name });
}

export function formatAssetScanResult(
  result: AssetSecurityScanResult,
  label: string,
  options: AssetSecurityPresentationOptions = {},
): string {
  if (result.findings.length === 0) {
    return `No security findings for ${label}.`;
  }

  const header = result.blocked
    ? (options.blockedHeader ?? `Blocked unsafe ${label}:`)
    : `Security findings for ${label}:`;

  const lines = result.findings.map(
    (finding) => `- [${finding.severity}] ${finding.code}: ${finding.message}`,
  );

  return `${header}\n${lines.join("\n")}`;
}

export function warningFindings(
  result: AssetSecurityScanResult,
): AssetSecurityFinding[] {
  return result.findings.filter((finding) => finding.severity === "warning");
}

export function toAssetSecurityScanRecord(
  result: AssetSecurityScanResult,
  scannedAt: string = new Date().toISOString(),
): AssetSecurityScanRecord {
  return {
    scanned_at: scannedAt,
    blocked: result.blocked,
    findings: result.findings,
  };
}

export function getAssetSecurityStatus(
  result: AssetSecurityScanResult | null,
  options: AssetSecurityPresentationOptions = {},
): { label: string; className: string } {
  if (!result) {
    return {
      label: "Unknown",
      className: "bg-bg-sidebar border-border-strong/40 text-text-muted",
    };
  }

  if (result.blocked) {
    return {
      label: options.blockedLabel ?? "Blocked",
      className: "bg-red-100 border-red-300/70 text-red-900",
    };
  }

  if (result.findings.length > 0) {
    return {
      label: "Warnings",
      className: "bg-amber-100 border-amber-400/70 text-amber-950",
    };
  }

  return {
    label: "Clean",
    className: "bg-emerald-100 border-emerald-300/70 text-emerald-900",
  };
}

export function getAssetSecurityNoticeClass(
  result: AssetSecurityScanResult | null,
): string {
  if (result?.blocked) {
    return "border-red-300/80 bg-red-50 text-red-950";
  }

  if (result?.findings.length) {
    return "border-amber-400/70 bg-amber-100 text-amber-950";
  }

  return "border-border-strong/40 bg-bg-input text-text-base";
}

export function getAssetSecurityDismissButtonClass(
  result: AssetSecurityScanResult | null,
): string {
  if (result?.blocked) {
    return "text-red-900/70 hover:text-red-950 transition-colors";
  }

  return "text-amber-900/70 hover:text-amber-950 transition-colors";
}
