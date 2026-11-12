/**
 * URL templates for catalogue MCP servers whose address differs per customer,
 * such as `https://<account-name>.app.spacelift.io/mcp`.
 *
 * A token is written `<name>`, where `name` is lowercase letters, digits and
 * single hyphens. A valid template (see `parseUrlTemplate`) puts each token
 * in a whole label of the host, before at least two fixed labels, and nowhere
 * else. The user supplies a value for each token, and a value must be a
 * single DNS label (letters, digits and inner hyphens, at most 63 characters).
 * Together these rules keep the template's own domain: a value cannot add a
 * dot, slash, `@`, `:`, `?` or `#`, so it cannot point the server, and the
 * OAuth token sent to it, at a different domain.
 *
 * Nothing here uses a regular expression built from a template, so a stored
 * template cannot make matching slow.
 */

const TOKEN = /<([a-z][a-z0-9]*(?:-[a-z0-9]+)*)>/g;
const WHOLE_TOKEN = /^<([a-z][a-z0-9]*(?:-[a-z0-9]+)*)>$/;
const DNS_LABEL = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;
const URL_PARTS = /^(https?:\/\/)([^/?#]*)(.*)$/;
const MAX_TOKENS = 4;
const MAX_TEMPLATE_LENGTH = 512;

/** A host label of a parsed template: fixed text, or a token to fill. */
type HostLabel = { text: string } | { token: string };

interface ParsedUrlTemplate {
  /** `https://` or `http://`. */
  scheme: string;
  hostLabels: HostLabel[];
  /** Everything after the host: port, path, query and fragment. */
  rest: string;
  tokens: string[];
}

/** The distinct token names in `text`, in order of first appearance. */
export function urlTemplateTokens(text: string): string[] {
  const seen: string[] = [];
  for (const match of text.matchAll(TOKEN)) {
    if (!seen.includes(match[1])) seen.push(match[1]);
  }
  return seen;
}

/**
 * Split a template into its parts, or return `null` when it is not a valid
 * template. Valid means: an http(s) URL of at most 512 characters, one to
 * four distinct tokens, each token a whole host label, the last two host
 * labels fixed, and no token or `<`/`>` anywhere else.
 */
export function parseUrlTemplate(template: string): ParsedUrlTemplate | null {
  if (template.length > MAX_TEMPLATE_LENGTH) return null;
  const parts = URL_PARTS.exec(template);
  if (!parts) return null;
  const [, scheme, host, rest] = parts;
  if (/[<>]/.test(rest) || host.includes("@") || host.includes(":")) return null;

  const tokens: string[] = [];
  const hostLabels: HostLabel[] = [];
  for (const label of host.split(".")) {
    const whole = WHOLE_TOKEN.exec(label);
    if (whole) {
      if (tokens.includes(whole[1])) return null;
      tokens.push(whole[1]);
      hostLabels.push({ token: whole[1] });
    } else if (DNS_LABEL.test(label)) {
      hostLabels.push({ text: label });
    } else {
      return null;
    }
  }
  if (tokens.length === 0 || tokens.length > MAX_TOKENS) return null;
  const tail = hostLabels.slice(-2);
  if (tail.length < 2 || tail.some((l) => "token" in l)) return null;
  return { scheme, hostLabels, rest, tokens };
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
 * Returns a problem instead when the template is not valid, or when a value
 * is missing or is not a DNS label, so the caller never saves a URL that
 * still holds a token.
 */
export function fillUrlTemplate(template: string, values: Record<string, string>): UrlTemplateFill {
  const parsed = parseUrlTemplate(template);
  if (!parsed) return { problem: "This server's address template is not valid." };
  for (const token of parsed.tokens) {
    const problem = urlTemplateValueProblem(token, (values[token] ?? "").trim());
    if (problem) return { problem };
  }
  const host = parsed.hostLabels
    .map((l) => ("token" in l ? (values[l.token] ?? "").trim() : l.text))
    .join(".");
  return { url: `${parsed.scheme}${host}${parsed.rest}` };
}

/**
 * Read the token values back out of a URL that was filled from `template`.
 *
 * Returns an empty string for every token when `url` does not have the
 * template's shape, for example after a hand edit of the saved file, so the
 * editor shows empty fields rather than a wrong guess.
 */
export function readUrlTemplateValues(template: string, url: string): Record<string, string> {
  const parsed = parseUrlTemplate(template);
  if (!parsed) return {};
  const empty = Object.fromEntries(parsed.tokens.map((t) => [t, ""]));
  const parts = URL_PARTS.exec(url);
  if (!parts) return empty;
  const [, scheme, host, rest] = parts;
  if (scheme !== parsed.scheme || rest !== parsed.rest) return empty;
  const labels = host.split(".");
  if (labels.length !== parsed.hostLabels.length) return empty;

  const values: Record<string, string> = { ...empty };
  for (let i = 0; i < labels.length; i++) {
    const expected = parsed.hostLabels[i];
    if ("text" in expected) {
      if (labels[i].toLowerCase() !== expected.text.toLowerCase()) return empty;
    } else if (DNS_LABEL.test(labels[i])) {
      values[expected.token] = labels[i];
    } else {
      return empty;
    }
  }
  return values;
}

/**
 * The template behind a saved server's URL, or `null` when it has none.
 *
 * Only a template in `known`, the bundled catalogue's templated URLs, counts.
 * A stored `_url_template` can come from a remote source or a cloud pull, and
 * honouring any template there would let that source present an arbitrary
 * host as a locked catalogue server. A server added before templates were
 * recorded still holds the raw template as its URL, so that URL counts too
 * when it is a known template.
 */
export function savedUrlTemplate(
  urlTemplate: unknown,
  url: string,
  known: readonly string[],
): string | null {
  if (typeof urlTemplate === "string" && known.includes(urlTemplate)) return urlTemplate;
  return known.includes(url) ? url : null;
}

/** The valid templated URLs among `urls`, for use as `savedUrlTemplate`'s `known`. */
export function knownUrlTemplates(urls: Array<string | null | undefined>): string[] {
  return urls.filter((u): u is string => typeof u === "string" && parseUrlTemplate(u) !== null);
}
