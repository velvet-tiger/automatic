/**
 * Analytics module — Amplitude wrapper
 *
 * Guards:
 *  1. API key    — analytics are disabled when `VITE_AMPLITUDE_API_KEY` is not
 *                  set. Leave it blank in .env to disable locally.
 *  2. Opt-out    — analytics are disabled when the user has turned off analytics
 *                  in Settings (`analytics_enabled: false`).
 *
 * All calls are no-ops when either guard is active, so callers never need to
 * check the state themselves.
 */

import * as amplitude from "@amplitude/analytics-browser";

// ─── State ───────────────────────────────────────────────────────────────────

let _initialized = false;
let _enabled = true; // runtime opt-out flag; toggled by setAnalyticsEnabled()

// ─── Init ─────────────────────────────────────────────────────────────────────

/**
 * Call once after the user profile is loaded.
 *
 * @param userId  Stable user identifier (clerk_id from profile)
 * @param enabled Whether the user has analytics enabled in settings
 */
export function initAnalytics(userId: string, enabled: boolean): void {
  _enabled = enabled;

  if (!_enabled) {
    return;
  }

  const apiKey = import.meta.env.VITE_AMPLITUDE_API_KEY as string | undefined;
  if (!apiKey) {
    return;
  }

  amplitude.init(apiKey, userId, {
    autocapture: {
      // Disable automatic page-view and interaction tracking — we track
      // manually so we have full control over event names and properties.
      pageViews: false,
      formInteractions: false,
      fileDownloads: false,
      sessions: true, // session tracking is useful; tracks DAU/WAU/MAU
      elementInteractions: false,
    },
  });

  _initialized = true;

  // Identify the user with stable properties
  const identifyEvent = new amplitude.Identify();
  identifyEvent.setOnce("first_seen", new Date().toISOString());
  amplitude.identify(identifyEvent);

  track("app_started");
}

/**
 * Update the opt-out flag at runtime (e.g. when user toggles the setting).
 * If analytics were previously initialized but the user opts out, Amplitude's
 * opt-out flag is set which stops all future uploads.
 */
export function setAnalyticsEnabled(enabled: boolean): void {
  _enabled = enabled;

  if (_initialized) {
    amplitude.setOptOut(!enabled);
  }
}

// ─── Tracking ─────────────────────────────────────────────────────────────────

/**
 * Track a named event with optional properties.
 * No-op when analytics are disabled or not yet initialized.
 */
export function track(
  eventName: string,
  properties?: Record<string, unknown>
): void {
  if (!shouldTrack()) return;
  amplitude.track(eventName, properties);
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

// ─── Internal helpers ─────────────────────────────────────────────────────────

function shouldTrack(): boolean {
  return _initialized && _enabled;
}
