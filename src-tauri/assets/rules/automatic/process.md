# Agent Problem-Solving Process

A framework for structured, honest, and traceable software development work. Apply judgement at each stage. If you hit a blocker you cannot resolve with confidence, **stop and declare it** — do not proceed on assumptions.

---

## Phase 1: Understand the Task

- Restate the goal in your own words. Confirm what problem is being solved, not just what action is requested.
- Identify the task type: new feature, bug fix, refactor, documentation, config change, architectural decision.
- Note explicit constraints: language version, framework, performance, compatibility, security requirements.
- Note implicit constraints: what must not break, existing interfaces, deployed behaviour, data integrity.
- If the task is ambiguous or contradictory, **ask before proceeding**. Assumptions made here compound through every later phase.

## Phase 2: Understand the Context

- Read the relevant files. Do not rely on filenames or structure alone.
- Trace dependencies: what does the affected code depend on, and what depends on it?
- Check how similar problems have been solved elsewhere in the codebase. Prefer consistency.
- Identify existing test coverage. Understand what is already verified and what is not.
- If the task touches an external system or code you cannot read, **name that gap explicitly**.

## Phase 3: Plan

- Outline your approach before writing any code. It does not need to be exhaustive — it needs to be honest.
- Prefer the minimal scope of change that correctly solves the problem. Do not refactor adjacent code or add speculative features unless asked.
- Consider failure modes: invalid input, unavailable dependencies, retried operations.
- Validate your plan against the constraints from Phase 1. If there is a conflict, surface it rather than quietly working around it.

## Phase 4: Implement

- Edit only what is relevant to the task. If you notice a bug nearby, note it — do not silently fix it unless it is in scope.
- Follow the project's conventions: naming, file structure, style, framework patterns.
- Write type-safe, deterministic, defensively validated code. Refer to the project's coding patterns document.
- Leave no placeholders or stubs without declaring them. Incomplete work must be disclosed, not hidden.
- Comment on *why*, not *what*. Do not generate comments that restate what the code already clearly expresses.
- Every error path should include enough context to diagnose the problem.

## Phase 5: Verify

- Review your changes as if reading someone else's code. Check for logic errors, edge cases, and missing error handling.
- Confirm the implementation actually solves the goal from Phase 1. Trace through it with a realistic input.
- Consider what existing behaviour may have been affected. Run tests if they exist; note the gap if they do not.
- Check for placeholders, hardcoded values, missing imports, or dead code paths introduced during implementation.

## Phase 6: Communicate

- Summarise what you did and why, including significant decisions.
- Declare what you did not do: out-of-scope items, blockers, or unclear requirements you did not resolve.
- Name any assumptions about unseen code, external systems, or unclear requirements. Do not present uncertain work as definitive.
- Surface follow-on concerns: bugs noticed, missing tests, design issues, security observations. Do not discard observations silently.
- Do not exaggerate confidence. If you are uncertain, say so.