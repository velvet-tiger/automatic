---
name: automatic-testing
description: Principles and patterns for writing effective tests. Use when writing unit, integration, or end-to-end tests, or reviewing test quality.
authors:
  - Automatic
---

# Writing Tests

Tests are executable specifications. They prove that code does what it claims, and protect against future regressions. A test suite you do not trust is worse than none — it creates false confidence.

## Principles

**Test behaviour, not implementation.** Tests should describe what the code does, not how it does it. If refactoring internals breaks a test without changing observable behaviour, the test is wrong.

**One reason to fail.** Each test should assert one logical outcome. Tests that check multiple unrelated things are hard to diagnose when they fail.

**Tests must be deterministic.** A test that sometimes passes and sometimes fails is not a test — it is noise. Eliminate all sources of non-determinism: random seeds, timestamps, network calls, file system state.

**Fast tests get run.** Slow tests get skipped. Keep unit tests under 50ms each. Use integration and end-to-end tests sparingly and deliberately.

---

## Test Types

### Unit tests
Test a single function or class in isolation. All dependencies are replaced with fakes, stubs, or mocks.

- Aim: verify logic, edge cases, and error handling
- Speed: milliseconds
- Coverage target: all public functions, all branches

### Integration tests
Test two or more real components working together. May use a real database, real file system, or real HTTP client against a test server.

- Aim: verify that components communicate correctly
- Speed: seconds
- Coverage target: critical paths and data flows

### End-to-end tests
Test the full system from the outside — as a user would interact with it.

- Aim: verify that key user journeys work
- Speed: slow (seconds to minutes)
- Coverage target: happy paths and critical error paths only

---

## Structure

Follow the **Arrange / Act / Assert** pattern in every test:

```
// Arrange — set up the state required for the test
// Act     — perform the single action being tested
// Assert  — verify the outcome
```

Name tests as full sentences describing behaviour:

```
// Good: "returns an empty list when no items match the filter"
// Bad:  "test_filter" or "testFilter"
```

---

## What to Test

- **Happy path** — the expected behaviour with valid inputs
- **Edge cases** — empty collections, zero values, maximum values, boundary conditions
- **Error cases** — invalid inputs, missing dependencies, network failure, permission denied
- **Invariants** — properties that must always hold regardless of input

## What Not to Test

- Private implementation details (test the public API)
- Third-party library behaviour
- Framework internals
- Trivial getters and setters with no logic

---

## Test Doubles

Use the simplest double that meets your needs:

| Type | Use when |
|------|----------|
| **Stub** | You need a dependency to return a specific value |
| **Fake** | You need a working but simplified implementation (e.g. in-memory database) |
| **Mock** | You need to verify that a specific call was made with specific arguments |
| **Spy** | You need to record calls without changing behaviour |

Avoid mocking what you do not own. Mock at the boundary of your system.

---

## Test Quality Checks

Before committing a test, verify:

- [ ] The test fails before the feature is implemented (if writing TDD)
- [ ] The test fails for the right reason when it fails
- [ ] Removing the production code makes the test fail
- [ ] The test name describes the behaviour, not the method
- [ ] There are no hardcoded sleeps or retries hiding flakiness
- [ ] External dependencies (network, time, randomness) are controlled

---

## Maintaining Tests

- Delete tests that no longer reflect the current design — do not comment them out
- When a bug is found, write a failing test that reproduces it before fixing it
- If a test is frequently skipped or disabled, treat that as a code smell
