---
name: automatic-debugging
description: Systematic process for diagnosing and resolving defects. Use when debugging failures, investigating errors, or reproducing issues.
authors:
  - Automatic
---

# Systematic Debugging

A disciplined process for diagnosing and resolving defects. Guessing wastes time. Systematic investigation finds root causes.

## The Process

### 1. Reproduce first
Before touching code, confirm you can reproduce the failure consistently. A defect you cannot reproduce reliably cannot be safely fixed.

- Identify the exact inputs, environment, and sequence that trigger the issue
- Determine if it is deterministic or intermittent
- Record the actual vs. expected behaviour precisely

### 2. Understand before fixing
Read the error message and stack trace fully. Do not skip lines. The first error is usually the cause; later errors are often consequences.

- Locate the exact file, line, and call that failed
- Read the surrounding code — not just the failing line
- Check recent changes: `git log --oneline -20`, `git diff HEAD~5`

### 3. Form a hypothesis
State a specific, testable hypothesis: *"I believe X is happening because Y."* Do not proceed without one.

- Hypotheses must be falsifiable — if you cannot test it, it is not a hypothesis
- Start with the simplest explanation consistent with the evidence
- Avoid hypothesising about multiple unrelated causes at once

### 4. Gather evidence
Prove or disprove the hypothesis with minimal, targeted changes.

```bash
# Add temporary logging at the point of failure
# Check actual values, not assumed values
# Use the debugger — step through, don't assume execution flow
```

- Add logging that reveals state, not just "got here" messages
- Use the smallest possible test case that reproduces the issue
- Check: inputs going in, outputs coming out, state at failure point

### 5. Fix the cause, not the symptom
Once the root cause is confirmed, fix it directly.

- Removing an assertion to stop a test failing is not a fix
- Adding a `null` guard around a crash is not a fix if the null should never occur
- If the fix feels like a workaround, it probably is — keep investigating

### 6. Verify and prevent regression
- Confirm the fix resolves the original reproduction case
- Add a test that would have caught this defect
- Consider whether the same class of bug exists elsewhere

---

## Common Patterns

### Intermittent failures
- Likely causes: race conditions, timing dependencies, uninitialized state, external service variability
- Strategy: add logging to capture state at the moment of failure; increase test runs; check for shared mutable state

### "It works on my machine"
- Likely causes: environment differences (OS, language version, dependencies, env vars, file paths, timezone)
- Strategy: diff the environments explicitly; check `.env`, lock files, system dependencies; reproduce in a container

### Regression (worked before, broken now)
- Start with `git bisect` to isolate the breaking commit
- Read that commit's diff with fresh eyes

### Null / undefined errors
- Find where the value is set, not where it is read
- Ask: should this value ever be null? If not, find why it is

### Performance degradation
- Measure before you optimise — identify the actual bottleneck with profiling data, not intuition
- See the `performance-profiling` skill

---

## What Not to Do

- **Do not comment out failing code** to make tests pass
- **Do not add `sleep` or retry loops** to hide timing issues
- **Do not ignore warnings** — they are often early indicators of the real problem
- **Do not fix multiple things at once** — you will not know which change resolved the issue
- **Do not assume** — verify every assumption with evidence
