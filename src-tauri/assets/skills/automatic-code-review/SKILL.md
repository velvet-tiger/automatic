---
name: automatic-code-review
description: How to conduct and receive effective code reviews. Use when reviewing a PR, requesting review, or improving review quality.
authors:
  - Automatic
---

# Code Review

Code review is a quality gate and a knowledge-sharing mechanism. Its purpose is to catch defects, surface design problems, and spread understanding — not to enforce style preferences or demonstrate superiority.

## Mindset

Review the code, not the person. Every comment should serve the goal of shipping better software. Assume good intent; ask questions before making accusations.

---

## What to Look For

### Correctness (highest priority)
- Does the code do what it claims to do?
- Are there edge cases that are not handled? (null, empty, overflow, concurrency)
- Are error states handled explicitly, or silently swallowed?
- Does it match the requirements or specification?

### Security
- Is user input validated and sanitised before use?
- Are secrets or credentials ever logged or exposed?
- Are permissions and authorisation checks in the right place?
- See the `security-review` skill for a full checklist

### Design
- Does the change fit the existing architecture, or does it introduce inconsistency?
- Is responsibility correctly assigned — does each function/class do one thing?
- Are dependencies pointing in the right direction?
- Would a future maintainer understand this without asking the author?

### Testability and tests
- Is the new code covered by tests?
- Do the tests verify behaviour, not implementation?
- Would a failing test give a clear signal about what broke?

### Performance
- Are there obvious algorithmic issues (N+1 queries, O(n²) in a hot path)?
- Are expensive operations cached where appropriate?
- For critical paths, is there evidence of measurement rather than assumption?

### Maintainability
- Are names clear and specific?
- Is duplication introduced that could be extracted?
- Are there comments that explain *why*, not just *what*?
- Is dead code removed?

---

## How to Comment

**Be specific.** Vague comments waste everyone's time.

```
// Bad:  "This could be better"
// Good: "This will execute a database query for every item in the list.
//        Consider fetching all items in a single query before the loop."
```

**Label the severity.** Not all comments are blockers.

- `[blocking]` — must be resolved before merge; correctness or security issue
- `[suggestion]` — improvement worth discussing; not required
- `[nit]` — minor style or preference; take it or leave it
- `[question]` — asking for understanding, not requesting a change

**Suggest, do not just criticise.** If you identify a problem, offer a direction for fixing it.

---

## Scope

Review what was changed. Do not block a PR because of pre-existing issues in surrounding code — create a separate ticket for those. Do not add scope to a PR during review.

---

## For Authors

Before requesting review:

- [ ] Self-review the diff first — catch your own obvious mistakes
- [ ] The PR description explains *why*, not just *what* changed
- [ ] Tests are included and passing
- [ ] No debug code, commented-out blocks, or TODOs left in without a ticket reference
- [ ] The change is as small as possible — large PRs produce low-quality reviews

When receiving feedback:

- Respond to every comment — either with a change or a rationale for not changing
- Do not silently resolve comments you disagree with; discuss them
- `[nit]` comments are optional — you do not need to justify ignoring them

---

## What Code Review Is Not

- A style guide enforcement tool (use a linter for that)
- A place to redesign the entire system
- A gatekeeping exercise
- A substitute for automated testing
