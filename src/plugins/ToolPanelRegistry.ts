/**
 * ToolPanelRegistry — Registry for tool-specific detail panels.
 *
 * Plugins can register custom panel components to render when their tool
 * is selected in the Projects view. This avoids hardcoding tool names
 * in ProjectToolDetailPanel.
 *
 * Usage:
 *   // In plugins/index.ts, add to PLUGINS array:
 *   { name: "my-tool", panel: MyPanel }
 *
 *   // In ProjectToolDetailPanel:
 *   const Panel = getToolPanel(entry.name);
 *   if (Panel) return <Panel {...props} />;
 */

import type { ReactNode, ComponentType } from "react";

export interface ToolPanelProps {
  projectDir: string;
  sidebar: ReactNode;
}

export type ToolPanelComponent = ComponentType<ToolPanelProps>;

const _registry = new Map<string, ToolPanelComponent>();

export function registerToolPanel(toolName: string, component: ToolPanelComponent): void {
  _registry.set(toolName, component);
}

export function getToolPanel(toolName: string): ToolPanelComponent | null {
  return _registry.get(toolName) ?? null;
}

export function hasToolPanel(toolName: string): boolean {
  return _registry.has(toolName);
}

export function unregisterToolPanel(toolName: string): void {
  _registry.delete(toolName);
}

export function clearToolPanelRegistry(): void {
  _registry.clear();
}