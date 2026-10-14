import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";

const AWK_SCRIPT = path.join(__dirname, "dpkg-owners.awk");
const work = mkdtempSync(path.join(tmpdir(), "dpkg-owners-"));

afterAll(() => rmSync(work, { recursive: true, force: true }));

// Recorded from `dpkg -S` on Ubuntu 22.04, plus the line shapes it prints
// for diversions and multi-arch packages.
const DPKG_OUTPUT = [
  "libglib2.0-0:amd64: /usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
  "libglib2.0-dev:amd64: /usr/lib/x86_64-linux-gnu/libgio-2.0.so",
  "libpcre3:amd64, libpcre3:i386: /usr/share/doc/libpcre3/copyright",
  "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.10",
  "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/xlibbar.so.1",
  "libgdk-pixbuf-2.0-0:amd64: /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-tiff.so",
  "local diversion from: /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-tiff.so",
  "local diversion to: /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/libpixbufloader-tiff.so.diverted",
  "diversion by dash from: /bin/sh",
  "diversion by dash to: /bin/sh.distrib",
  "libdup-a:amd64: /usr/lib/x86_64-linux-gnu/libdup.so.1",
  "libdup-b:amd64: /usr/lib/libdup.so.1",
  "",
].join("\n");

const fixture = path.join(work, "dpkg-output.txt");
writeFileSync(fixture, DPKG_OUTPUT);

function owners(suffix: string): string[] {
  const out = execFileSync("awk", ["-v", `suffix=${suffix}`, "-f", AWK_SCRIPT, fixture], { encoding: "utf8" });
  return [...new Set(out.split("\n").filter((line) => line !== ""))].sort();
}

describe("dpkg-owners.awk", () => {
  it("finds the package for a flattened library and drops the architecture", () => {
    expect(owners("/libglib-2.0.so.0")).toEqual(["libglib2.0-0"]);
  });

  it("lists every package on a multi-arch line once", () => {
    expect(owners("/libpcre3/copyright")).toEqual(["libpcre3"]);
  });

  it("matches whole path segments only", () => {
    expect(owners("/libfoo.so.1")).toEqual([]);
    expect(owners("/libbar.so.1")).toEqual([]);
  });

  it("ignores local diversion lines and keeps the owning package", () => {
    expect(owners("/2.10.0/loaders/libpixbufloader-tiff.so")).toEqual(["libgdk-pixbuf-2.0-0"]);
    expect(owners("/libpixbufloader-tiff.so.diverted")).toEqual([]);
  });

  it("ignores package diversion lines", () => {
    expect(owners("/bin/sh")).toEqual([]);
    expect(owners("/bin/sh.distrib")).toEqual([]);
  });

  it("reports both packages when two own files with the same suffix", () => {
    expect(owners("/libdup.so.1")).toEqual(["libdup-a", "libdup-b"]);
  });

  it("prints nothing for a file no package owns", () => {
    expect(owners("/libmissing.so.1")).toEqual([]);
  });
});
