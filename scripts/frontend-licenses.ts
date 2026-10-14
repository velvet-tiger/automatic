// Vite plugin that writes third-party-licenses.txt into the build output: the
// licence of every npm package whose code ends up in the interface bundle.
// The release workflow ships the file inside each app bundle, and the app
// shows it under Settings > Support.
//
// The list comes from the modules Rollup actually rendered into the output
// chunks, not from package.json, so a dependency that is tree-shaken away is
// not listed and code pulled in by a dependency is.
//
// The build fails when a package has no licence file, or when its declared
// licence is not accepted below. A new licence is then a decision someone
// makes, not something that slips in.

import { lstatSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import type { Plugin } from "vite";

export const OUTPUT_FILE = "third-party-licenses.txt";

// Licences a package may carry. An expression joined with OR is accepted when
// one of its parts is. One joined with AND is accepted when every part is.
const ACCEPTED_LICENCES = new Set([
  "MIT",
  "ISC",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "0BSD",
]);

// Code that is emitted into the bundle without being a module Rollup can see.
// Tailwind writes its preflight and utility CSS from inside the compiler that
// @tailwindcss/vite runs, so no module id points into its package.
const EMITTED_WITHOUT_MODULE = ["tailwindcss"];

// Vite's own helpers (module preload polyfill, dynamic import preloading) are
// virtual modules with ids like "\0vite/preload-helper.js".
const VITE_VIRTUAL_PREFIX = "\0vite/";

// The Tauri plugin packages ship only LICENSE.spdx, which names the licences
// and the copyright holder but holds no licence text. They come from the same
// project as @tauri-apps/api, under the same copyright line and the same
// licence files, so those files are printed for them as well.
const LICENCE_TEXT_FROM: Record<string, string> = {
  "@tauri-apps/plugin-dialog": "@tauri-apps/api",
  "@tauri-apps/plugin-opener": "@tauri-apps/api",
  "@tauri-apps/plugin-updater": "@tauri-apps/api",
};

// Vite's LICENSE.md holds Vite's own licence followed by the licences of the
// dependencies Vite bundles into its build tool. Only Vite's own code reaches
// the app, so the file is cut at this heading. The build fails if the heading
// disappears, so the cut is checked again after a Vite upgrade.
const LICENCE_FILE_CUT_AT: Record<string, string> = {
  vite: "\n# Licenses of bundled dependencies",
};

const LICENCE_FILE_PATTERN = /^(licen[cs]e|copying|notice)/i;

const SEPARATOR = "=".repeat(80);

interface PackageManifest {
  name: string;
  version: string;
  license: string;
}

interface LicensedPackage extends PackageManifest {
  licenceFiles: { name: string; text: string }[];
}

/**
 * Returns the directory of the npm package that holds a Rollup module, or
 * null when the module is not inside node_modules (the app's own source, or
 * a virtual module). Query strings and the "\0" prefix of virtual ids are
 * removed first.
 */
export function packageRootFromModuleId(id: string): string | null {
  const clean = id.replace(/^\0/, "").split("?")[0].replace(/\\/g, "/");
  const marker = "/node_modules/";
  const index = clean.lastIndexOf(marker);
  if (index === -1) return null;
  const afterMarker = clean.slice(index + marker.length).split("/");
  const segments = afterMarker[0].startsWith("@") ? 2 : 1;
  if (afterMarker.length <= segments || afterMarker.slice(0, segments).some((s) => s === "")) {
    return null;
  }
  return clean.slice(0, index + marker.length) + afterMarker.slice(0, segments).join("/");
}

/**
 * True when an SPDX licence expression is allowed by ACCEPTED_LICENCES. An
 * expression that mixes AND and OR is rejected so that someone reads it.
 */
export function isAcceptedLicence(expression: string): boolean {
  const flat = expression.replace(/[()]/g, " ").trim().replace(/\s+/g, " ");
  if (flat === "") return false;
  const hasOr = flat.includes(" OR ");
  const hasAnd = flat.includes(" AND ");
  if (hasOr && hasAnd) return false;
  if (hasOr) return flat.split(" OR ").some((part) => ACCEPTED_LICENCES.has(part));
  if (hasAnd) return flat.split(" AND ").every((part) => ACCEPTED_LICENCES.has(part));
  return ACCEPTED_LICENCES.has(flat);
}

function readManifest(root: string): PackageManifest {
  const file = path.join(root, "package.json");
  const parsed: unknown = JSON.parse(readFileSync(file, "utf8"));
  if (typeof parsed !== "object" || parsed === null) {
    throw new Error(`${file}: not a JSON object`);
  }
  const { name, version, license } = parsed as Record<string, unknown>;
  if (typeof name !== "string" || typeof version !== "string") {
    throw new Error(`${file}: missing "name" or "version"`);
  }
  if (typeof license !== "string") {
    throw new Error(`${name} ${version}: package.json has no "license" string`);
  }
  return { name, version, license };
}

function readLicenceFiles(root: string, packageName: string): { name: string; text: string }[] {
  const cutAt = LICENCE_FILE_CUT_AT[packageName];
  return readdirSync(root)
    .filter((entry) => {
      if (!LICENCE_FILE_PATTERN.test(entry)) return false;
      const stats = lstatSync(path.join(root, entry));
      // The file ships in the app. A symlink could pull in a file from
      // outside the package, so it stops the build instead of being followed.
      if (stats.isSymbolicLink()) {
        throw new Error(`${packageName}: ${entry} is a symlink. Licence files must be regular files.`);
      }
      return stats.isFile();
    })
    .sort()
    .map((entry) => {
      const text = readFileSync(path.join(root, entry), "utf8").trimEnd();
      if (cutAt === undefined) return { name: entry, text };
      const index = text.indexOf(cutAt);
      if (index === -1) {
        throw new Error(
          `${packageName}: ${entry} no longer contains "${cutAt.trim()}". Check what the file holds and update LICENCE_FILE_CUT_AT in scripts/frontend-licenses.ts.`,
        );
      }
      return { name: entry, text: text.slice(0, index).trimEnd() };
    });
}

/** Resolves a package by name from the project root, for packages found without a module id. */
function resolvePackageRoot(projectRoot: string, name: string): string {
  return path.join(projectRoot, "node_modules", ...name.split("/"));
}

function collectPackage(root: string, projectRoot: string): LicensedPackage {
  const manifest = readManifest(root);
  if (!isAcceptedLicence(manifest.license)) {
    throw new Error(
      `${manifest.name} ${manifest.version} is licensed "${manifest.license}", which scripts/frontend-licenses.ts does not accept. Decide whether it may ship, then add it to ACCEPTED_LICENCES.`,
    );
  }
  const textFrom = LICENCE_TEXT_FROM[manifest.name];
  const licenceFiles = [
    ...readLicenceFiles(root, manifest.name),
    ...(textFrom === undefined ? [] : readLicenceFiles(resolvePackageRoot(projectRoot, textFrom), textFrom)),
  ];
  // LICENSE.spdx alone names a licence but holds none of its text.
  if (!licenceFiles.some((file) => !file.name.toLowerCase().endsWith(".spdx"))) {
    throw new Error(
      `${manifest.name} ${manifest.version} ships no licence text. Find where its licence text is published and add an entry to LICENCE_TEXT_FROM in scripts/frontend-licenses.ts.`,
    );
  }
  return { ...manifest, licenceFiles };
}

/** Renders the notices file from a list of packages, sorted by name and version. */
export function renderLicences(packages: LicensedPackage[]): string {
  const sorted = [...packages].sort(
    (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
  );
  const header = [
    "Licences of the third-party code in Automatic's interface",
    "=========================================================",
    "",
    "The build of Automatic's interface wrote this file. It lists every npm",
    "package whose code is in the interface, with that package's licence files.",
    "",
    "Each package's source is at the npmjs.com address beside its name.",
  ].join("\n");
  const sections = sorted.map((pkg) => {
    const files = pkg.licenceFiles.map((file) => `--- ${file.name} ---\n\n${file.text}`).join("\n\n");
    return [
      SEPARATOR,
      `${pkg.name} ${pkg.version} (${pkg.license})`,
      `https://www.npmjs.com/package/${pkg.name}/v/${pkg.version}`,
      "",
      files,
    ].join("\n");
  });
  return `${header}\n\n${sections.join("\n\n")}\n`;
}

// CSS imported from JavaScript is listed in the chunk with no rendered
// length, because its text goes to a CSS asset instead.
const STYLE_MODULE_PATTERN = /\.(css|scss|sass|less|styl|stylus)(\?|$)/;

/** The parts of a Rollup output entry that the licence list reads. */
export type BundleEntry =
  | { type: "chunk"; modules: Record<string, { renderedLength: number }> }
  | { type: "asset"; originalFileNames: readonly string[] };

/**
 * Writes the notices text for a build: every npm package with code in the
 * output chunks, CSS or other assets taken from a package, Tailwind's
 * generated CSS, and Vite when its helpers are in the bundle. Throws when a
 * package fails the licence checks or when no package is found at all.
 */
export function collectLicences(projectRoot: string, bundle: Iterable<BundleEntry>): string {
  const roots = new Set<string>();
  let usesViteHelpers = false;
  for (const output of bundle) {
    if (output.type === "asset") {
      // Vite gives asset names relative to the project root
      // ("node_modules/pkg/font.woff2"), so they are made absolute first.
      for (const fileName of output.originalFileNames) {
        const root = packageRootFromModuleId(path.resolve(projectRoot, fileName));
        if (root !== null) roots.add(root);
      }
      continue;
    }
    for (const [id, info] of Object.entries(output.modules)) {
      if (info.renderedLength === 0 && !STYLE_MODULE_PATTERN.test(id)) continue;
      if (id.startsWith(VITE_VIRTUAL_PREFIX)) {
        usesViteHelpers = true;
        continue;
      }
      const root = packageRootFromModuleId(id);
      if (root !== null) roots.add(root);
    }
  }
  if (roots.size === 0) {
    throw new Error("No npm packages were found in the bundle. The licence list would be empty.");
  }
  const extra = usesViteHelpers ? [...EMITTED_WITHOUT_MODULE, "vite"] : EMITTED_WITHOUT_MODULE;
  for (const name of extra) roots.add(resolvePackageRoot(projectRoot, name));

  const byKey = new Map<string, LicensedPackage>();
  for (const root of roots) {
    const pkg = collectPackage(root, projectRoot);
    byKey.set(`${pkg.name}@${pkg.version}`, pkg);
  }
  return renderLicences([...byKey.values()]);
}

/** The Vite plugin. It runs only for `vite build`. */
export function thirdPartyLicences(): Plugin {
  let projectRoot = process.cwd();
  return {
    name: "automatic-third-party-licences",
    apply: "build",
    configResolved(config) {
      projectRoot = config.root;
    },
    generateBundle(_options, bundle) {
      this.emitFile({
        type: "asset",
        fileName: OUTPUT_FILE,
        source: collectLicences(projectRoot, Object.values(bundle)),
      });
    },
  };
}
