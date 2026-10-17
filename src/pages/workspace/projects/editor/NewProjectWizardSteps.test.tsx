import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  WizardGroupStep,
  WizardProfileStep,
  WizardToolsStep,
  WizardSettingsStep,
  continueLabel,
  profilesProvidedByGroups,
} from "./NewProjectWizardSteps";
import type { ProjectProfile, ProjectToolEntry } from "../types";

function profile(name: string, description = ""): ProjectProfile {
  return {
    name,
    description,
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    user_agents: [],
    user_commands: [],
    hooks: [],
    rules: [],
  };
}

function tool(name: string, projectScoped = true): ProjectToolEntry {
  return {
    name,
    display_name: `Tool ${name}`,
    description: `${name} description`,
    url: "",
    kind: "cli",
    detected: null,
    provides_tab: false,
    project_scoped: projectScoped,
  };
}

describe("continueLabel", () => {
  it("reads Skip until something is chosen", () => {
    expect(continueLabel(0)).toBe("Skip");
    expect(continueLabel(2)).toBe("Continue");
  });
});

describe("WizardGroupStep", () => {
  const base = { loading: false, onBack: vi.fn(), onNext: vi.fn() };

  it("adds and removes a group from the selection", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <WizardGroupStep {...base} groups={["alpha", "beta"]} selected={[]} onChange={onChange} />
    );
    fireEvent.click(screen.getByRole("checkbox", { name: /alpha/ }));
    expect(onChange).toHaveBeenLastCalledWith(["alpha"]);

    rerender(
      <WizardGroupStep {...base} groups={["alpha", "beta"]} selected={["alpha", "beta"]} onChange={onChange} />
    );
    fireEvent.click(screen.getByRole("checkbox", { name: /alpha/ }));
    expect(onChange).toHaveBeenLastCalledWith(["beta"]);
  });

  it("says so when no groups exist, and still lets the user skip", () => {
    const onNext = vi.fn();
    render(<WizardGroupStep {...base} groups={[]} selected={[]} onChange={vi.fn()} onNext={onNext} />);
    expect(screen.getByText(/No groups exist yet/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Skip/ }));
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it("shows a loading note while groups load", () => {
    render(<WizardGroupStep {...base} loading groups={[]} selected={[]} onChange={vi.fn()} />);
    expect(screen.getByText("Loading groups…")).toBeInTheDocument();
  });
});

describe("WizardProfileStep", () => {
  it("shows descriptions and marks the selected profile", () => {
    render(
      <WizardProfileStep
        profiles={[profile("web", "Web stack"), profile("data")]}
        providedByGroup={{}}
        selected={["web"]}
        onChange={vi.fn()}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    expect(screen.getByText("Web stack")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /web/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("checkbox", { name: /data/ })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("button", { name: /Continue/ })).toBeInTheDocument();
  });

  it("explains that no profiles exist", () => {
    render(
      <WizardProfileStep
        profiles={[]}
        providedByGroup={{}}
        selected={[]}
        onChange={vi.fn()}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    expect(screen.getByText(/No profiles yet/)).toBeInTheDocument();
  });
});

describe("profilesProvidedByGroups", () => {
  it("lists the profiles of the selected groups only", () => {
    const groupProfiles = { team: ["web"], other: ["data"] };
    expect(profilesProvidedByGroups(["team"], groupProfiles)).toEqual({ web: "team" });
    expect(profilesProvidedByGroups([], groupProfiles)).toEqual({});
  });

  it("credits a shared profile to the first group by name", () => {
    const groupProfiles = { zeta: ["web"], alpha: ["web", "data"] };
    expect(profilesProvidedByGroups(["zeta", "alpha"], groupProfiles)).toEqual({
      web: "alpha",
      data: "alpha",
    });
  });

  it("ignores a selected group with no loaded profiles", () => {
    expect(profilesProvidedByGroups(["missing"], {})).toEqual({});
  });
});

describe("WizardProfileStep with group-provided profiles", () => {
  it("shows them ticked and locked with the group name, and leaves others free", () => {
    const onChange = vi.fn();
    render(
      <WizardProfileStep
        profiles={[profile("web"), profile("data")]}
        providedByGroup={{ web: "team" }}
        selected={[]}
        onChange={onChange}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    const web = screen.getByRole("checkbox", { name: /web/ });
    expect(screen.getByText("From group: team")).toBeInTheDocument();
    expect(web).toHaveAttribute("aria-checked", "true");
    expect(web).toBeDisabled();
    fireEvent.click(web);
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("checkbox", { name: /data/ }));
    expect(onChange).toHaveBeenCalledWith(["data"]);
  });

  it("still shows a group profile that is missing from the library list", () => {
    render(
      <WizardProfileStep
        profiles={[]}
        providedByGroup={{ gone: "team" }}
        selected={[]}
        onChange={vi.fn()}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    expect(screen.getByRole("checkbox", { name: /gone/ })).toBeDisabled();
    expect(screen.queryByText(/No profiles yet/)).not.toBeInTheDocument();
  });

  it("reads Continue, not Skip, when a group already provides a profile", () => {
    render(
      <WizardProfileStep
        profiles={[profile("web")]}
        providedByGroup={{ web: "team" }}
        selected={[]}
        onChange={vi.fn()}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    expect(screen.getByRole("button", { name: /Continue/ })).toBeInTheDocument();
  });
});

describe("WizardToolsStep", () => {
  it("hides machine-wide tools and badges detected ones", () => {
    render(
      <WizardToolsStep
        entries={[tool("a"), tool("b"), tool("machine", false)]}
        loading={false}
        detected={["b"]}
        selected={[]}
        onChange={vi.fn()}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    expect(screen.getByText("Tool a")).toBeInTheDocument();
    expect(screen.getByText("Tool b")).toBeInTheDocument();
    expect(screen.queryByText("Tool machine")).not.toBeInTheDocument();
    expect(screen.getAllByText("Detected")).toHaveLength(1);
  });

  it("toggles a tool", () => {
    const onChange = vi.fn();
    render(
      <WizardToolsStep
        entries={[tool("a")]}
        loading={false}
        detected={[]}
        selected={["x"]}
        onChange={onChange}
        onBack={vi.fn()}
        onNext={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole("checkbox", { name: /Tool a/ }));
    expect(onChange).toHaveBeenCalledWith(["x", "a"]);
  });
});

describe("WizardSettingsStep", () => {
  it("reports each setting change as a patch", () => {
    const onChange = vi.fn();
    render(
      <WizardSettingsStep settings={{}} onChange={onChange} onBack={vi.fn()} onCreate={vi.fn()} />
    );
    fireEvent.click(screen.getByRole("button", { name: "Silent" }));
    expect(onChange).toHaveBeenLastCalledWith({ mode: "silent" });

    fireEvent.click(screen.getByRole("switch", { name: "Manage .gitignore for this project" }));
    expect(onChange).toHaveBeenLastCalledWith({ manage_gitignore: true });

    fireEvent.click(screen.getByRole("switch", { name: "Read only for new checkouts" }));
    expect(onChange).toHaveBeenLastCalledWith({ read_only: true });
  });

  it("turns a setting back off", () => {
    const onChange = vi.fn();
    render(
      <WizardSettingsStep
        settings={{ mode: "silent", manage_gitignore: true, read_only: true }}
        onChange={onChange}
        onBack={vi.fn()}
        onCreate={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole("switch", { name: "Manage .gitignore for this project" }));
    expect(onChange).toHaveBeenLastCalledWith({ manage_gitignore: false });
    fireEvent.click(screen.getByRole("button", { name: "Normal" }));
    expect(onChange).toHaveBeenLastCalledWith({ mode: "normal" });
  });

  it("creates the project from the last step", () => {
    const onCreate = vi.fn();
    render(<WizardSettingsStep settings={{}} onChange={vi.fn()} onBack={vi.fn()} onCreate={onCreate} />);
    fireEvent.click(screen.getByRole("button", { name: /Create Project/ }));
    expect(onCreate).toHaveBeenCalledTimes(1);
  });
});
