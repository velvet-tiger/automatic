---
name: automatic-security-review
description: Security review checklist and threat mindset for any codebase. Use when reviewing code for vulnerabilities, implementing auth, or handling user input.
authors:
  - Automatic
---

# Security Review

Security defects are expensive to fix after deployment and can cause irreversible harm. Review for security at every stage — during design, code review, and before release.

## Threat Mindset

When reviewing code, ask: *what happens if an attacker controls this input?* Assume all external data is hostile until proven otherwise. External data includes: HTTP request parameters, headers, cookies, file uploads, database values, environment variables, inter-service messages, and anything read from the network or file system.

---

## Input Validation

- **Validate at the boundary** — check and sanitise all external input as soon as it enters the system
- **Allowlist, not blocklist** — define what is permitted; reject everything else
- **Validate type, length, format, and range** — all four, not just one
- **Never trust client-side validation alone** — it can be bypassed; replicate checks server-side

---

## Injection

Injection vulnerabilities occur when untrusted data is interpreted as code.

**SQL injection**
- Use parameterised queries or prepared statements; never concatenate user input into SQL
- ORMs reduce risk but do not eliminate it — watch for raw query escape hatches

**Command injection**
- Avoid passing user data to shell commands
- If unavoidable, use argument arrays (not string interpolation) and allowlist permitted values

**Template injection**
- Do not render user-supplied strings as templates
- Escape output in templates; separate data from structure

**Path traversal**
- Resolve and validate file paths against an allowlisted base directory before use
- Reject paths containing `..` or absolute path components

---

## Authentication and Authorisation

- **Authenticate** — verify *who* is making the request
- **Authorise** — verify they are *permitted* to perform the action on *that specific resource*
- Both checks must happen on the server side on every request
- Check authorisation at the resource level, not just the route level (e.g. user A must not be able to access user B's record by guessing an ID)
- Prefer short-lived tokens; rotate secrets on compromise

---

## Secrets Management

- **Never hardcode secrets** — no API keys, passwords, or tokens in source code
- **Never log secrets** — check that logging statements do not capture auth headers, tokens, or credentials
- **Use environment variables or a secrets manager** — not config files committed to the repo
- **Rotate secrets regularly** and immediately on suspected exposure
- **Audit `.gitignore`** — ensure `.env` and credential files are excluded

---

## Cryptography

- Do not implement your own cryptography — use established, audited libraries
- Use current recommended algorithms: AES-256-GCM for symmetric encryption, RSA-4096 or Ed25519 for asymmetric, bcrypt/scrypt/Argon2 for password hashing
- Never use MD5 or SHA-1 for security purposes
- Generate random values using a cryptographically secure RNG

---

## Dependencies

- Keep dependencies up to date — most CVEs have patches available on the day of disclosure
- Run a dependency vulnerability scanner regularly (`npm audit`, `cargo audit`, `pip-audit`, etc.)
- Minimise the number of dependencies — each one is an attack surface
- Pin versions in production; review changelogs on updates

---

## Error Handling

- Return generic error messages to external callers — do not expose stack traces, internal paths, or system information
- Log full details internally for diagnosis
- Treat failed security checks as hard stops — do not attempt to recover and continue

---

## Common Vulnerabilities Checklist

| Category | Check |
|---|---|
| Input validation | All external inputs validated and sanitised |
| SQL | Parameterised queries used throughout |
| Auth | Authentication and authorisation on every protected endpoint |
| Authorisation | Resource-level access checks (not just route-level) |
| Secrets | No secrets in source, logs, or error responses |
| Cryptography | Industry-standard algorithms; no homebrew crypto |
| Dependencies | No known CVEs in dependency tree |
| Error handling | Internal errors do not leak to external callers |
| File paths | Paths resolved and validated against an allowlist |
| Rate limiting | Sensitive endpoints (login, signup, password reset) rate-limited |
