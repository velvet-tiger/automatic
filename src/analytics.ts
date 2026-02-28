/**
 * Analytics module — thin wrapper over the Rust track_event command.
 *
 * Events are sent from Rust (reqwest → Amplitude HTTP API v2) so that:
 *   - The API key never appears in the JS bundle.
 *   - We are not subject to WKWebView network quirks (sendBeacon / fetch).
 *
 * Guards:
 *   1. Opt-out  — analytics are skipped when the user has disabled them in
 *                 Settings (`analytics_enabled: false`). The Rust layer also
 *                 enforces this, but we short-circuit here to avoid the IPC hop.
 *   2. API key  — if no key was compiled into the Rust binary the call is a
 *                 silent no-op on the Rust side.
 *
 * All public functions are fire-and-forget — they never throw.
 */

import { invoke } from "@tauri-apps/api/core";

// ─── State ────────────────────────────────────────────────────────────────────

let _userId: string = "anonymous";
let _enabled: boolean = true;
let _initialized: boolean = false;

// ─── Init ─────────────────────────────────────────────────────────────────────

/**
 * Call once after the user profile and settings are loaded.
 *
 * @param userId  Stable user identifier (clerk_id from profile)
 * @param enabled Whether the user has analytics enabled in settings
 */
export async function initAnalytics(userId: string, enabled: boolean): Promise<void> {
  _userId = userId;
  _enabled = enabled;
  _initialized = true;

  if (!_enabled) {
    console.info("[analytics] disabled by user setting");
    return;
  }

  // Fire the first event to confirm the pipeline is working.
  await _send("app_started");
}

/**
 * Update the opt-out flag at runtime (e.g. when the user toggles the setting).
 */
export function setAnalyticsEnabled(enabled: boolean): void {
  _enabled = enabled;
}

// ─── Tracking ─────────────────────────────────────────────────────────────────

/**
 * Track a named event with optional properties. Fire-and-forget — never throws.
 */
export function track(eventName: string, properties?: Record<string, unknown>): void {
  _send(eventName, properties);
}

// ─── Typed event helpers ──────────────────────────────────────────────────────

// Navigation
export function trackNavigation(tab: string): void {
  track("navigation_tab_clicked", { tab });
}

// Skills
export function trackSkillCreated(name: string, source: "local" | "remote"): void {
  track("skill_created", { name, source });
}

export function trackSkillUpdated(name: string): void {
  track("skill_updated", { name });
}

export function trackSkillDeleted(name: string): void {
  track("skill_deleted", { name });
}

export function trackSkillSynced(name: string): void {
  track("skill_synced", { name });
}

export function trackAllSkillsSynced(count: number): void {
  track("skills_synced_all", { count });
}

export function trackSkillInstalled(name: string, source_url?: string): void {
  track("skill_installed_remote", { name, source_url });
}

// Projects
export function trackProjectCreated(name: string): void {
  track("project_created", { name });
}

export function trackProjectUpdated(
  name: string,
  meta: { agent_count: number; skill_count: number; mcp_count: number }
): void {
  track("project_updated", { name, ...meta });
}

export function trackProjectDeleted(name: string): void {
  track("project_deleted", { name });
}

export function trackProjectSynced(name: string): void {
  track("project_synced", { name });
}

export function trackProjectAgentAdded(projectName: string, agentName: string): void {
  track("project_agent_added", { project: projectName, agent: agentName });
}

export function trackProjectAgentRemoved(projectName: string, agentName: string): void {
  track("project_agent_removed", { project: projectName, agent: agentName });
}

export function trackProjectSkillAdded(projectName: string, skillName: string): void {
  track("project_skill_added", { project: projectName, skill: skillName });
}

export function trackProjectSkillRemoved(projectName: string, skillName: string): void {
  track("project_skill_removed", { project: projectName, skill: skillName });
}

export function trackProjectMcpServerAdded(projectName: string, serverName: string): void {
  track("project_mcp_server_added", { project: projectName, server: serverName });
}

export function trackProjectMcpServerRemoved(projectName: string, serverName: string): void {
  track("project_mcp_server_removed", { project: projectName, server: serverName });
}

// MCP Servers
export function trackMcpServerCreated(name: string): void {
  track("mcp_server_created", { name });
}

export function trackMcpServerUpdated(name: string): void {
  track("mcp_server_updated", { name });
}

export function trackMcpServerDeleted(name: string): void {
  track("mcp_server_deleted", { name });
}

// Settings
export function trackSettingChanged(key: string, value: unknown): void {
  track("setting_changed", { key, value });
}

// Updates
export function trackUpdateChecked(result: "available" | "not_available" | "error"): void {
  track("update_checked", { result });
}

export function trackUpdateInstalled(version: string): void {
  track("update_installed", { version });
}

// Onboarding
export function identifyOnboarding(onboarding: {
  role: string;
  aiUsage: string;
  agents: string[];
}): void {
  track("onboarding_completed", {
    role: onboarding.role,
    ai_usage: onboarding.aiUsage,
    agents: onboarding.agents,
  });
}

// ─── Internal ─────────────────────────────────────────────────────────────────

async function _send(event: string, properties?: Record<string, unknown>): Promise<void> {
  if (!_initialized || !_enabled) return;

  try {
    await invoke("track_event", {
      userId: _userId,
      event,
      properties: properties ?? null,
      enabled: _enabled,
    });
  } catch (e) {
    // Analytics must never crash the app.
    console.warn("[analytics] track_event invoke failed:", e);
  }
}
