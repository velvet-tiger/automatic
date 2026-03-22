/**
 * Plugin configuration — list all plugins with their optional panels.
 *
 * Add new plugins by adding an entry to the PLUGINS array.
 * Each plugin has:
 *   - name: tool identifier (must match Backend tool name)
 *   - panel?: React component for the detail view (optional)
 */

import type { ToolPanelComponent } from "./ToolPanelRegistry";
import { registerToolPanel } from "./ToolPanelRegistry";

import { SpecKittyPanel } from "./spec-kitty/SpecKittyPanel";

interface PluginEntry {
  name: string;
  panel?: ToolPanelComponent;
}

const PLUGINS: PluginEntry[] = [
  { name: "spec-kitty", panel: SpecKittyPanel },
];

export function initPlugins(): void {
  for (const plugin of PLUGINS) {
    if (plugin.panel) {
      registerToolPanel(plugin.name, plugin.panel);
    }
  }
}

export { getToolPanel, hasToolPanel } from "./ToolPanelRegistry";