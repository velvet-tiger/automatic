import { describe, it, expect } from "vitest";
import {
  buildPageTree,
  childFolderPath,
  folderPaths,
  isLastPageInFolder,
  joinFrontmatter,
  newPageContent,
  pageName,
  pagePath,
  splitFrontmatter,
} from "./pageTree";

describe("page tree", () => {
  const pages = ["index.md", "guides/setup.md", "guides/deploying.md", "guides/trouble/dns.md", "decisions/0001-ledger.md"];

  it("builds nested folders from paths", () => {
    const tree = buildPageTree(pages);
    expect(tree.pages).toEqual(["index.md"]);
    expect(tree.folders.map((f) => f.name)).toEqual(["Decisions", "Guides"]);
    const guides = tree.folders[1]!;
    expect(guides.pages.map(pageName)).toEqual(["Deploying", "Setup"]);
    expect(guides.folders[0]!.path).toBe("guides/trouble");
  });

  it("lists every folder path", () => {
    expect(folderPaths(pages)).toEqual(["decisions", "guides", "guides/trouble"]);
  });

  it("makes valid, unique page paths from titles", () => {
    expect(pagePath("guides", "Setup guide!", pages)).toBe("guides/setup-guide.md");
    expect(pagePath("guides", "Setup", pages)).toBe("guides/setup-2.md");
    expect(pagePath("", "   ", [])).toBe("untitled.md");
    expect(childFolderPath("guides", "How To")).toBe("guides/how-to");
  });

  it("knows when a page is the last in its folder", () => {
    expect(isLastPageInFolder("decisions/0001-ledger.md", pages)).toBe(true);
    expect(isLastPageInFolder("guides/setup.md", pages)).toBe(false);
    expect(isLastPageInFolder("index.md", pages)).toBe(false);
  });

  it("keeps frontmatter out of the editor and puts it back on save", () => {
    const content = newPageContent("Setup");
    const { frontmatter, body } = splitFrontmatter(content);
    expect(frontmatter).toContain('title: "Setup"');
    expect(body).toBe("# Setup\n");
    expect(joinFrontmatter(frontmatter, body)).toBe(content);
    expect(splitFrontmatter("# No header\n")).toEqual({ frontmatter: "", body: "# No header\n" });
  });
});
