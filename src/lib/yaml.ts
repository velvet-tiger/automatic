/**
 * Escape a string for use inside a YAML double-quoted scalar.
 * Backslashes must be escaped before double quotes so that the
 * backslashes added here are not themselves double-escaped.
 */
export function escapeYamlDoubleQuoted(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
