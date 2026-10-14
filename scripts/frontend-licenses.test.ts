import { describe, expect, it } from "vitest";
import { isAcceptedLicence, packageRootFromModuleId, renderLicences } from "./frontend-licenses";

describe("packageRootFromModuleId", () => {
  it("finds an unscoped package", () => {
    expect(packageRootFromModuleId("/app/node_modules/react/cjs/react.production.js")).toBe(
      "/app/node_modules/react",
    );
  });

  it("finds a scoped package", () => {
    expect(packageRootFromModuleId("/app/node_modules/@tauri-apps/api/core.js")).toBe(
      "/app/node_modules/@tauri-apps/api",
    );
  });

  it("uses the innermost node_modules for a nested copy", () => {
    expect(packageRootFromModuleId("/app/node_modules/a/node_modules/b/index.js")).toBe(
      "/app/node_modules/a/node_modules/b",
    );
  });

  it("strips the virtual prefix and query string", () => {
    expect(packageRootFromModuleId("\0/app/node_modules/react/index.js?commonjs-proxy")).toBe(
      "/app/node_modules/react",
    );
  });

  it("normalises Windows separators", () => {
    expect(packageRootFromModuleId("C:\\app\\node_modules\\react\\index.js")).toBe(
      "C:/app/node_modules/react",
    );
  });

  it("returns null for the app's own source and for virtual modules", () => {
    expect(packageRootFromModuleId("/app/src/main.tsx")).toBeNull();
    expect(packageRootFromModuleId("\0vite/preload-helper.js")).toBeNull();
  });

  it("returns null when the path ends at node_modules or a bare scope", () => {
    expect(packageRootFromModuleId("/app/node_modules/")).toBeNull();
    expect(packageRootFromModuleId("/app/node_modules/@scope")).toBeNull();
  });
});

describe("isAcceptedLicence", () => {
  it("accepts a single accepted licence", () => {
    expect(isAcceptedLicence("MIT")).toBe(true);
  });

  it("accepts an OR expression when one part is accepted", () => {
    expect(isAcceptedLicence("(GPL-3.0-only OR MIT)")).toBe(true);
  });

  it("rejects an AND expression when one part is not accepted", () => {
    expect(isAcceptedLicence("MIT AND GPL-3.0-only")).toBe(false);
    expect(isAcceptedLicence("MIT AND ISC")).toBe(true);
  });

  it("rejects mixed AND and OR, unknown and empty expressions", () => {
    expect(isAcceptedLicence("MIT OR ISC AND GPL-3.0-only")).toBe(false);
    expect(isAcceptedLicence("GPL-3.0-only")).toBe(false);
    expect(isAcceptedLicence("")).toBe(false);
  });
});

describe("renderLicences", () => {
  it("sorts packages and prints each licence file", () => {
    const text = renderLicences([
      { name: "zeta", version: "1.0.0", license: "MIT", licenceFiles: [{ name: "LICENSE", text: "Z" }] },
      { name: "alpha", version: "2.0.0", license: "ISC", licenceFiles: [{ name: "LICENSE", text: "A" }] },
    ]);
    expect(text.indexOf("alpha 2.0.0 (ISC)")).toBeLessThan(text.indexOf("zeta 1.0.0 (MIT)"));
    expect(text).toContain("https://www.npmjs.com/package/alpha/v/2.0.0");
    expect(text).toContain("--- LICENSE ---\n\nA");
  });
});
