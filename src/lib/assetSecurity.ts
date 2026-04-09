import { invoke } from "@tauri-apps/api/core";

export type AssetScanKind =
  | "skill"
  | "skill_manifest"
  | "companion_file"
  | "user_command"
  | "user_agent"
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
): string {
  if (result.findings.length === 0) {
    return `No security findings for ${label}.`;
  }

  const header = result.blocked
    ? `Blocked unsafe ${label}:`
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
