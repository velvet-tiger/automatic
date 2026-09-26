const XML_TAG_RE = /<[^>]+>/;
const RESERVED_WORDS = ["anthropic", "claude"];
const NAME_CHARSET_RE = /^[a-z0-9-]*$/;

/**
 * Validate the front matter `name` of a skill or sub-agent.
 * Returns a user-facing error message, or `null` when the name is valid.
 * The rules follow the Agent Skills name spec: at most 64 characters of
 * lowercase letters, digits, and hyphens, with no reserved vendor words.
 */
export function validateLibraryName(value: string): string | null {
  if (!value) return "Name is required.";
  if (value.length > 64) return "Name must be 64 characters or fewer.";
  if (!NAME_CHARSET_RE.test(value)) return "Name may only contain lowercase letters, numbers, and hyphens.";
  if (XML_TAG_RE.test(value)) return "Name must not contain XML tags.";
  for (const word of RESERVED_WORDS) {
    if (value === word || value.startsWith(word + "-") || value.endsWith("-" + word) || value.includes("-" + word + "-")) {
      return `Name must not contain the reserved word "${word}".`;
    }
  }
  return null;
}
