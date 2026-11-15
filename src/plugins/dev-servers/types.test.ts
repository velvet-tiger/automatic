import { describe, expect, it } from "vitest";
import {
  applyDetectedPackageManager,
  describeServerCommand,
  packageManagerBadge,
  packageManagerLabel,
} from "./types";

describe("describeServerCommand", () => {
  it("shows a package manager script as the command it runs", () => {
    expect(describeServerCommand({ package_manager: "pnpm", script: "dev" })).toBe("pnpm run dev");
  });

  it("shows a custom command as typed", () => {
    expect(describeServerCommand({ package_manager: "command", script: "make serve" })).toBe("make serve");
  });
});

describe("packageManagerLabel", () => {
  it("names the custom command choice and keeps package manager names", () => {
    expect(packageManagerLabel("command")).toBe("Custom command");
    expect(packageManagerLabel("yarn")).toBe("yarn");
  });
});

describe("packageManagerBadge", () => {
  it("tags a custom command as custom", () => {
    expect(packageManagerBadge("command")).toBe("custom");
    expect(packageManagerBadge("npm")).toBe("npm");
  });
});

describe("applyDetectedPackageManager", () => {
  it("keeps a custom command when a lockfile is detected", () => {
    expect(applyDetectedPackageManager("command", "pnpm")).toBe("command");
  });

  it("takes the detected package manager otherwise", () => {
    expect(applyDetectedPackageManager("npm", "yarn")).toBe("yarn");
  });
});
