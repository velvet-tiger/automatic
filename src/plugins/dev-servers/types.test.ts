import { describe, expect, it } from "vitest";
import { describeServerCommand, packageManagerLabel } from "./types";

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
