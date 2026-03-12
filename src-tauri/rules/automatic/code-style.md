# Good Coding Patterns

These patterns apply to all code you write or meaningfully modify. When touching existing code, apply these patterns to the code you change — do not silently leave surrounding violations in place, but do not refactor unrelated code without being asked.

## 1. Explicit Typing and Interfaces

- Always specify function signatures, parameter types, and return types.
- Use interfaces or abstract classes to define clear contracts between components.
- Prefer the strictest type available; avoid `any`, `mixed`, or untyped generics unless genuinely necessary.

## 2. Immutable Data and Pure Functions

- Avoid side effects unless required by the task.
- Prefer immutable data structures and functional patterns where possible.
- Clearly separate functions that read from those that write; do not mix both in a single unit without good reason.

## 3. Composition Over Inheritance

- Favour composing behaviour through injected dependencies and interfaces over deep inheritance hierarchies.
- Inheritance is appropriate for genuine "is-a" relationships with shared invariants — not for code reuse alone.
- Keep class hierarchies shallow; more than two levels of concrete inheritance is a signal to reconsider.

## 4. Consistent Naming and Domain Semantics

- Use meaningful, domain-relevant names (e.g., `PatientRepository` instead of `DataHandler`).
- Avoid abbreviations, internal shorthand, or generic names like `Manager`, `Helper`, or `Util`.
- Names should reflect intent and domain vocabulary, not implementation details.

## 5. Dependency Injection and Separation of Concerns

- Never hardcode dependencies. Inject via constructors or configuration.
- Keep business logic distinct from infrastructure (I/O, persistence, transport).
- A class should have one clear reason to change.

## 6. Error Handling with Context

- Catch only expected, specific exceptions — not broad base types unless you have a clear reason.
- When rethrowing, include context (what was being attempted, relevant identifiers) and preserve the original cause.
- Do not swallow errors silently. If an error is ignored intentionally, document why.

## 7. Idempotency and Determinism

- Operations with side effects (I/O, DB writes, API calls, event publishing, schema migrations) must be safe to re-run with the same inputs.
- Design APIs and event handlers with idempotency in mind, not just individual functions.
- Avoid nondeterministic behaviour (random values, timestamps, unordered collections in sensitive paths) unless it is explicitly required and documented.

## 8. Defensive Programming

- Validate all inputs and assumptions at system boundaries (API surfaces, queue consumers, public class interfaces).
- Fail fast and loudly when contracts are violated — do not silently degrade or return a default that masks the error.
- Trust nothing from outside the current process boundary without validation.

## 9. Security-Aware Defaults

- Never hardcode secrets, credentials, or environment-specific values. Use environment variables or a secrets manager.
- Sanitise and validate all external input before use, regardless of source.
- Apply the principle of least privilege: request only the access the code actually needs.
- When in doubt about a security implication, flag it with a comment rather than proceeding silently.

## 10. Testability and Verifiability

- Write code that can be unit-tested independently of infrastructure.
- Avoid static singletons, global state, or hidden dependencies that impede testing.
- If a piece of logic is difficult to test in isolation, that is a signal the design needs revisiting.

## 11. Small, Focused Units

- Prefer small, single-purpose functions and classes over large, multi-concern ones.
- If a function requires significant explanation to describe what it does, it is probably doing too much.
- Do not over-generate: produce only the code required for the task. Avoid speculative abstractions or unused extension points.

## 12. Documentation and Intent

- Every public class and function should declare its purpose, inputs, outputs, and any side effects.
- Comments should explain *why* a decision was made, not restate *what* the code does — the code already says what it does.
- Do not generate comments that add no information beyond what is immediately obvious from the code.

## 13. Conformance to Environment

- Before generating code, identify the project's language version, framework conventions, linting configuration, and deployment targets by reading existing files (e.g., `composer.json`, `Cargo.toml`, `package.json`, `.eslintrc`, `phpstan.neon`).
- Match the dominant patterns and style already present in the codebase — consistency with the surrounding code takes precedence over personal preference.
- If the environment cannot be determined and it materially affects the output, ask before proceeding.