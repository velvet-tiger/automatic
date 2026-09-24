// Pure helpers for documentation pages. Pages are stored by path
// (`guides/setup.md`), as in the webapp: folders exist only as path
// prefixes, so a folder with no pages does not exist. The UI shows plain
// names and never paths or `.md`.

export interface PageFolder {
  /** Full folder path, "" for the top level. */
  path: string;
  name: string;
  folders: PageFolder[];
  /** Full page paths directly in this folder. */
  pages: string[];
}

/** Turn a title into a valid path segment (`Setup guide` → `setup-guide`). */
export function segmentFromTitle(title: string): string {
  const segment = title
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[^a-z0-9]+/, "")
    .replace(/-+$/, "")
    .replace(/\.+/g, ".")
    .slice(0, 64);
  return segment || "untitled";
}

/** A display name for a path segment (`setup-guide.md` → `Setup guide`). */
export function displayName(segment: string): string {
  const base = segment.replace(/\.md$/, "").replace(/[-_]+/g, " ").trim();
  return base ? base.charAt(0).toUpperCase() + base.slice(1) : segment;
}

export function pageName(path: string): string {
  return displayName(path.split("/").pop() ?? path);
}

export function parentFolder(path: string): string {
  const i = path.lastIndexOf("/");
  return i === -1 ? "" : path.slice(0, i);
}

/** Every folder path that holds a page, directly or deeper, sorted. */
export function folderPaths(pages: string[]): string[] {
  const folders = new Set<string>();
  for (const page of pages) {
    const parts = page.split("/").slice(0, -1);
    for (let i = 1; i <= parts.length; i++) folders.add(parts.slice(0, i).join("/"));
  }
  return [...folders].sort();
}

/** `folder/segment.md`, with a numeric suffix if that path is taken. */
export function pagePath(folder: string, title: string, existing: string[]): string {
  const taken = new Set(existing);
  const prefix = folder ? `${folder}/` : "";
  const base = segmentFromTitle(title);
  let candidate = `${prefix}${base}.md`;
  let i = 2;
  while (taken.has(candidate)) candidate = `${prefix}${base}-${i++}.md`;
  return candidate;
}

/** A folder path for a new folder named `name` inside `parent`. */
export function childFolderPath(parent: string, name: string): string {
  const segment = segmentFromTitle(name);
  return parent ? `${parent}/${segment}` : segment;
}

export function buildPageTree(pages: string[]): PageFolder {
  const root: PageFolder = { path: "", name: "", folders: [], pages: [] };
  const byPath = new Map<string, PageFolder>([["", root]]);
  const ensure = (path: string): PageFolder => {
    const found = byPath.get(path);
    if (found) return found;
    const parent = ensure(parentFolder(path));
    const folder: PageFolder = { path, name: displayName(path.split("/").pop() ?? path), folders: [], pages: [] };
    parent.folders.push(folder);
    byPath.set(path, folder);
    return folder;
  };
  for (const page of [...pages].sort()) ensure(parentFolder(page)).pages.push(page);
  const sort = (folder: PageFolder) => {
    folder.folders.sort((a, b) => a.name.localeCompare(b.name));
    folder.pages.sort((a, b) => pageName(a).localeCompare(pageName(b)));
    folder.folders.forEach(sort);
  };
  sort(root);
  return root;
}

/** True when `page` is the only page left in its folder (and any parents it keeps alive). */
export function isLastPageInFolder(page: string, pages: string[]): boolean {
  const folder = parentFolder(page);
  if (!folder) return false;
  return !pages.some((p) => p !== page && p.startsWith(`${folder}/`));
}

/**
 * Split a page into its YAML frontmatter block (kept as written, including
 * the `---` lines) and the body the editor shows.
 */
export function splitFrontmatter(content: string): { frontmatter: string; body: string } {
  const match = /^---\r?\n[\s\S]*?\r?\n---\r?\n?/.exec(content);
  if (!match) return { frontmatter: "", body: content };
  return { frontmatter: match[0], body: content.slice(match[0].length).replace(/^\r?\n/, "") };
}

export function joinFrontmatter(frontmatter: string, body: string): string {
  if (!frontmatter) return body;
  const head = frontmatter.endsWith("\n") ? frontmatter : `${frontmatter}\n`;
  return `${head}\n${body}`;
}

/** New page content: webapp-compatible frontmatter plus a heading. */
export function newPageContent(title: string): string {
  const safeTitle = title.replace(/\r?\n/g, " ").trim() || "Untitled";
  return `---\ntype: page\ntitle: ${JSON.stringify(safeTitle)}\n---\n\n# ${safeTitle}\n`;
}
