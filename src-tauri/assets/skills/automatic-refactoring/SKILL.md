---
name: automatic-refactoring
description: Techniques for improving code structure without changing behaviour. Use when cleaning up code, reducing complexity, or addressing technical debt.
authors:
  - Automatic
---

# Refactoring

Refactoring is the process of changing code structure without changing its observable behaviour. The goal is to make the code easier to understand and modify. Refactoring is not rewriting, and it is not adding features.

## The Rule

**Never refactor and change behaviour at the same time.** If you are changing what code does and how it is structured in the same commit, you cannot tell which change introduced a regression. Do one thing at a time.

---

## When to Refactor

Refactor when:
- You are about to add a feature and the existing structure makes it harder than it should be
- You have just fixed a bug and the code that caused it is unclear
- Code review reveals that a change is hard to understand
- You are working in an area and find it difficult to follow

Do not refactor when:
- There are no tests — add tests first, or you cannot verify you preserved behaviour
- You are under time pressure to ship a fix — stabilise first, improve later
- You do not understand what the code does — read it first

---

## Recognising the Need

### Code smells that warrant refactoring

**Long function** — a function that does more than one thing. Extract the parts into named functions.

**Long parameter list** — more than 3–4 parameters usually means the function is doing too much, or the parameters belong to an object.

**Duplicated code** — the same logic appearing in two or more places. Extract and reuse.

**Deep nesting** — more than 2–3 levels of indentation signals branching complexity. Use early returns, extract conditions, or restructure.

**Inconsistent naming** — variables or functions named differently for the same concept. Pick one name and use it everywhere.

**Magic numbers and strings** — unexplained literals. Extract to named constants.

**Large class** — a class that has too many responsibilities. Split into focused classes.

**Feature envy** — a function that uses more data from another class than its own. Move it.

---

## Common Refactoring Moves

### Extract function
Take a block of code with a clear purpose and move it into a named function.

```
// Before: comment explains what the next 10 lines do
// After: a function whose name replaces the comment
```

### Inline function
When a function's body is as clear as its name, remove the indirection.

### Rename
Rename when the name does not accurately describe what the thing is or does. Good names eliminate the need for comments.

### Extract variable
Replace a complex expression with a named variable that explains what it represents.

### Replace condition with early return
Invert nested conditions to reduce indentation.

```
// Before: if (valid) { ... long block ... }
// After:  if (!valid) return; ... logic at top level ...
```

### Replace magic number with constant
```
// Before: if (retries > 3)
// After:  if (retries > MAX_RETRY_ATTEMPTS)
```

### Move function or data
When a function uses more context from another module than its own, move it there.

---

## Process

1. **Verify tests pass** before starting
2. **Make one change at a time** — the smallest possible structural move
3. **Run tests after each change** — confirm behaviour is preserved
4. **Commit each change separately** — keeps the history readable and reversible
5. **Stop when the code is clear enough** for the next task — do not over-engineer

---

## What Refactoring Is Not

- **Rewriting** — if you are replacing the algorithm, that is a behaviour change
- **Adding features** — if you are adding new capability while restructuring, separate them
- **Optimising** — performance changes alter observable behaviour (timing); measure separately
- **Cosmetic formatting** — use an auto-formatter for that; do not mix with structural changes
