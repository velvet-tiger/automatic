# Good Coding Patterns

## 1. Explicit Typing and Interfaces
- Always specify function signatures and return types.
- Use interfaces or abstract classes to define clear contracts between components.

## 2. Immutable Data and Pure Functions
- Avoid side effects unless required.
- Prefer immutable data structures and functional patterns where possible.

## 3. Consistent Naming and Domain Semantics
- Use meaningful, domain-relevant names (e.g., `PatientRepository` instead of `DataHandler`).
- Avoid abbreviations or internal shorthand.

## 4. Dependency Injection and Separation of Concerns
- Never hardcode dependencies. Inject via constructors or configuration.
- Keep business logic distinct from infrastructure.

## 5. Error Handling with Context
- Catch only expected exceptions.
- When rethrowing, include context and preserve the original cause.

## 6. Idempotency and Determinism
- Functions performing side effects (I/O, DB updates) must be safe to re-run.
- Avoid nondeterministic behaviour unless necessary.

## 7. Defensive Programming
- Validate all inputs and assumptions.
- Fail fast and loudly when contracts are violated.

## 8. Testability and Verifiability
- Write code that can be unit-tested independently.
- Avoid static singletons or external state that impede testing.

## 9. Documentation and Intent
- Every public class and function should declare purpose, inputs, outputs, and side effects.
- Comments should explain *why*, not *what*.

## 10. Conformance to Environment
- Match the project’s coding standards, linting, framework conventions, and deployment targets.
- If unsure, ask or detect automatically before generating code.