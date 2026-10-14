import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type BundleEntry,
  collectLicences,
  isAcceptedLicence,
  packageRootFromModuleId,
  renderLicences,
} from "./frontend-licenses";

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

describe("collectLicences", () => {
  let root: string;

  function addPackage(name: string, license: string, files: Record<string, string>): string {
    const dir = path.join(root, "node_modules", ...name.split("/"));
    mkdirSync(dir, { recursive: true });
    writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name, version: "1.0.0", license }));
    for (const [file, text] of Object.entries(files)) writeFileSync(path.join(dir, file), text);
    return dir;
  }

  function chunk(modules: Record<string, number>): BundleEntry {
    const entries = Object.entries(modules).map(([id, renderedLength]) => [id, { renderedLength }] as const);
    return { type: "chunk", modules: Object.fromEntries(entries) };
  }

  beforeEach(() => {
    root = mkdtempSync(path.join(tmpdir(), "frontend-licences-"));
    // Always added: Tailwind's CSS is emitted without a module id.
    addPackage("tailwindcss", "MIT", { LICENSE: "Copyright (c) Tailwind Labs" });
  });

  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it("lists packages with rendered code and skips tree-shaken ones", () => {
    const used = addPackage("used", "MIT", { LICENSE: "used licence" });
    const unused = addPackage("unused", "MIT", { LICENSE: "unused licence" });
    const text = collectLicences(root, [chunk({ [`${used}/index.js`]: 10, [`${unused}/index.js`]: 0 })]);
    expect(text).toContain("used 1.0.0 (MIT)");
    expect(text).toContain("tailwindcss 1.0.0 (MIT)");
    expect(text).not.toContain("unused 1.0.0");
  });

  it("lists CSS modules and assets that come from a package", () => {
    const css = addPackage("some-css", "MIT", { LICENSE: "css licence" });
    addPackage("@fonts/inter", "OFL-1.1 OR MIT", { LICENSE: "font licence" });
    const text = collectLicences(root, [
      chunk({ [`${css}/style.css`]: 0 }),
      // Vite names assets relative to the project root.
      { type: "asset", originalFileNames: ["node_modules/@fonts/inter/inter.woff2"] },
    ]);
    expect(text).toContain("some-css 1.0.0 (MIT)");
    expect(text).toContain("@fonts/inter 1.0.0 (OFL-1.1 OR MIT)");
  });

  it("adds Vite, cut at its bundled-dependency licences, when its helpers are used", () => {
    const used = addPackage("used", "MIT", { LICENSE: "used licence" });
    addPackage("vite", "MIT", {
      "LICENSE.md": "# Vite core license\nVite MIT text\n\n# Licenses of bundled dependencies\nother text",
    });
    const text = collectLicences(root, [chunk({ [`${used}/index.js`]: 1, "\0vite/preload-helper.js": 1 })]);
    expect(text).toContain("vite 1.0.0 (MIT)");
    expect(text).toContain("Vite MIT text");
    expect(text).not.toContain("other text");
  });

  it("fails when Vite's licence file loses the heading it is cut at", () => {
    const used = addPackage("used", "MIT", { LICENSE: "used licence" });
    addPackage("vite", "MIT", { "LICENSE.md": "# Vite core license\nno heading here" });
    expect(() => collectLicences(root, [chunk({ [`${used}/index.js`]: 1, "\0vite/preload-helper.js": 1 })])).toThrow(
      /LICENCE_FILE_CUT_AT/,
    );
  });

  it("borrows the licence text of @tauri-apps/api for the Tauri plugin packages", () => {
    addPackage("@tauri-apps/api", "Apache-2.0 OR MIT", { LICENSE_MIT: "Tauri Apps Contributors MIT" });
    const plugin = addPackage("@tauri-apps/plugin-opener", "MIT OR Apache-2.0", { "LICENSE.spdx": "spdx only" });
    const text = collectLicences(root, [chunk({ [`${plugin}/index.js`]: 5 })]);
    expect(text).toMatch(/@tauri-apps\/plugin-opener 1\.0\.0[\s\S]*spdx only[\s\S]*Tauri Apps Contributors MIT/);
  });

  it("fails on a licence that is not accepted", () => {
    const gpl = addPackage("gpl-thing", "GPL-3.0-only", { LICENSE: "GPL" });
    expect(() => collectLicences(root, [chunk({ [`${gpl}/index.js`]: 1 })])).toThrow(/does not accept/);
  });

  it("fails on a package with no licence text", () => {
    const bare = addPackage("bare", "MIT", {});
    expect(() => collectLicences(root, [chunk({ [`${bare}/index.js`]: 1 })])).toThrow(/ships no licence text/);
  });

  it("fails on a licence file that is a symlink", () => {
    const linked = addPackage("linked", "MIT", {});
    writeFileSync(path.join(root, "outside.txt"), "not a licence");
    symlinkSync(path.join(root, "outside.txt"), path.join(linked, "LICENSE"));
    expect(() => collectLicences(root, [chunk({ [`${linked}/index.js`]: 1 })])).toThrow(/is a symlink/);
  });

  it("fails when the bundle holds no npm package", () => {
    expect(() => collectLicences(root, [chunk({ [`${root}/src/main.tsx`]: 100 })])).toThrow(/No npm packages/);
  });
});
