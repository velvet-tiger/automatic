// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { AgentSelector } from "../../../../../components/AgentSelector";
import type { AgentOptions } from "../../../../../components/AgentSelector";
import type { AgentInfo, Project } from "../../types";

interface AgentsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  availableAgents: AgentInfo[];
  addItem: (field: "skills" | "mcp_servers" | "providers" | "agents", value: string) => Promise<boolean>;
  handleRemoveAgent: (idx: number) => void | Promise<void>;
}

export function AgentsPanel({ project, setProject, setDirty, availableAgents, addItem, handleRemoveAgent }: AgentsPanelProps) {
  return (
    <section>
      <AgentSelector
        agentIds={project.agents}
        availableAgents={availableAgents}
        onAdd={(id) => addItem("agents", id)}
        onRemove={(i) => handleRemoveAgent(i)}
        emptyMessage="No agent tools selected. Add tools to enable config sync."
        agentOptions={project.agent_options}
        onOptionChange={(agentId, patch) => {
          const current: AgentOptions = project.agent_options?.[agentId] ?? { claude_rules_in_dot_claude: true };
          setProject({
            ...project,
            agent_options: {
              ...(project.agent_options ?? {}),
              [agentId]: { ...current, ...patch },
            },
            updated_at: new Date().toISOString(),
          });
          setDirty(true);
        }}
      />
    </section>
  );
}
