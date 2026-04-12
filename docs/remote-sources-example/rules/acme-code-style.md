# Acme Code Style

## TypeScript

- Strict mode enabled. No `any` types.
- Use `interface` for object shapes, `type` for unions and intersections.
- Prefer `const` over `let`. Never use `var`.
- All functions must have explicit return types.
- Use named exports. Default exports are only for page components.

## Naming

- Components: `PascalCase` (e.g. `UserProfile`)
- Hooks: `camelCase` with `use` prefix (e.g. `useProductSearch`)
- Utilities: `camelCase` (e.g. `formatCurrency`)
- Constants: `UPPER_SNAKE_CASE` (e.g. `MAX_RETRY_COUNT`)
- Files: match the primary export name

## Formatting

- 2-space indentation
- Single quotes for strings
- Trailing commas in multi-line constructs
- No semicolons (handled by Prettier)

## Error Handling

- Never swallow errors silently.
- All API calls must handle error responses explicitly.
- Use Acme's `AppError` class for domain errors.
- Log errors with `logger.error()`, never `console.error()`.
