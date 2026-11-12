/**
 * URL templates for catalogue MCP servers whose address differs per customer,
 * such as `https://<account-name>.app.spacelift.io/mcp`.
 *
 * A token is written `<name>`, where `name` is lowercase letters, digits and
 * single hyphens. The user supplies a value for each token. A value must be a
 * single DNS label (letters, digits and inner hyphens, at most 63 characters).
 * That rule keeps the template's own host: a value cannot add a dot, slash,
 * `@`, `:`, `?` or `#`, so it cannot point the server, and the OAuth token
 * sent to it, at a different domain.
 */

const TOKEN_SOURCE = "<([a-z][a-z0-9]*(?:-[a-z0-9]+)*)>";
const DNS_LABEL = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;

/** The distinct token names in `template`, in order of first appearance. */
export function urlTemplateTokens(template: string): string[] {
  const seen: string[] = [];
  for (const match of template.matchAll(new RegExp(TOKEN_SOURCE, "g"))) {
    if (!seen.includes(match[1])) seen.push(match[1]);
  }
  return seen;
}

/** A field label for a token: `account-name` becomes `Account name`. */
export function urlTemplateTokenLabel(token: string): string {
  const words = token.split("-").join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Why `value` cannot fill a token, or `null` when it can. */
export function urlTemplateValueProblem(token: string, value: string): string | null {
  const label = urlTemplateTokenLabel(token);
  if (!value) return `Enter your ${label.toLowerCase()}.`;
  if (!DNS_LABEL.test(value)) {
    return `${label} can contain only letters, digits and hyphens, and cannot start or end with a hyphen.`;
  }
  return null;
}

export type UrlTemplateFill = { url: string } | { problem: string };

/**
 * Replace every token in `template` with its trimmed value.
 *
 * Returns the first problem instead when a value is missing or is not a DNS
 * label, so the caller never saves a URL that still holds a token.
 */
export function fillUrlTemplate(template: string, values: Record<string, string>): UrlTemplateFill {
  for (const token of urlTemplateTokens(template)) {
    const problem = urlTemplateValueProblem(token, (values[token] ?? "").trim());
    if (problem) return { problem };
  }
  const url = template.replace(new RegExp(TOKEN_SOURCE, "g"), (_, token: string) =>
    (values[token] ?? "").trim(),
  );
  return { url };
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Read the token values back out of a URL that was filled from `template`.
 *
 * Returns an empty string for every token when `url` does not have the
 * template's shape, for example after a hand edit of the saved file, so the
 * editor shows empty fields rather than a wrong guess.
 */
export function readUrlTemplateValues(template: string, url: string): Record<string, string> {
  const tokens = urlTemplateTokens(template);
  const empty = Object.fromEntries(tokens.map((t) => [t, ""]));
  const groups: string[] = [];
  const pattern = template
    .split(new RegExp(TOKEN_SOURCE))
    .map((part, i) => {
      if (i % 2 === 0) return escapeRegExp(part);
      if (groups.includes(part)) return `\\k<g${groups.indexOf(part)}>`;
      groups.push(part);
      return `(?<g${groups.length - 1}>[^./?#@:<>]*)`;
    })
    .join("");
  const match = new RegExp(`^${pattern}$`).exec(url);
  if (!match?.groups) return empty;
  const values = { ...empty };
  groups.forEach((token, i) => {
    values[token] = match.groups?.[`g${i}`] ?? "";
  });
  return values;
}

/**
 * The template behind a saved server's URL, or `null` when it has none.
 *
 * A server added before templates were recorded still holds the raw template
 * as its URL. Treating that URL as the template lets the user repair it.
 */
export function savedUrlTemplate(urlTemplate: unknown, url: string): string | null {
  if (typeof urlTemplate === "string" && urlTemplateTokens(urlTemplate).length > 0) return urlTemplate;
  return urlTemplateTokens(url).length > 0 ? url : null;
}
