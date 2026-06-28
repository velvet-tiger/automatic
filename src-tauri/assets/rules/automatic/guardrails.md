# Engineering Guardrails

Each rule names a specific moment — the point where a bad pattern is about to be written, a false claim is about to be made, or a shortcut is about to be taken. When that moment arrives, stop and apply the rule. The red flag at the end of each rule is the signal that you're in that moment.

## 1. Before you write `any`, `unknown` without narrowing, `mixed`, `object`, or an untyped parameter — stop.

Write the actual type. At system boundaries (JSON bodies, external APIs, CLI args, env vars) use a parser (Zod, io-ts, schema validation) that produces a typed result; do not `as`-cast past a boundary. Internal code should never need `any`.

**Red flag:** writing `as any`, `as unknown as T`, `declare const x: any`. If you can't write the real type, the design is wrong.

## 2. Before you write a function that both reads and mutates — split it.

One function queries, another applies. If you can't name the function without the word "and" ("getAndSave", "loadOrCreate", "fetchThenUpdate"), it's doing two things. Side-effecting operations (I/O, DB, network) live at the edges; pure transformations in the middle.

**Red flag:** a function that returns a value AND changes global/DB/disk state.

## 3. Before you write `class X extends Y` — check if you're reusing code.

If yes, compose instead: pass Y as a constructor arg. Inheritance is only for genuine is-a relationships. Never go more than two concrete levels deep.

**Red flag:** "abstract" base classes with concrete children whose only shared behaviour is a couple of methods.

## 4. Before you name something `Manager`, `Helper`, `Util`, `Service`, `Handler`, `Processor`, `Controller` — stop.

Reach for the domain noun: `OrderRepository`, `InvoiceRenderer`, `SearchIndexCache`. Generic names hide missing concepts.

**Red flag:** a class whose only cohesion is the suffix; e.g. `UploadManager` doing parsing, upload, and caching.

## 5. Before you write `app(Thing::class)`, `new Client()`, `import { globalState }` inside business logic — stop.

Dependencies arrive via constructor parameters or function arguments. Hardcoded instantiation ties the code to its environment and kills testability.

**Red flag:** a unit test that can't run because a downstream call reaches for a service locator or global.

## 6. Before you write `catch (\Throwable)`, `catch (Exception)`, `catch (_)`, `except:` — stop.

Catch the specific types you actually expect to handle. Broad catches mask bugs. When rethrowing, wrap with context: what was being attempted, the input id, the upstream call that failed. Don't swallow errors silently; if you're going to ignore one, document why in the catch body.

**Red flag:** `// silence` or an empty catch block.

## 7. Before you write anything that hits an external system — check idempotency.

Every side-effecting operation (DB write, API call, event publish, queue send, schema migration) must be safe to re-run with the same inputs. If your design can't guarantee that — because e.g. it auto-increments or generates random IDs inline — fix the design or document the non-idempotency.

**Red flag:** a handler that behaves differently on retry than on first call, with no comment explaining why.

## 8. Before you silently default a missing/invalid input — stop.

Missing env var → throw at boot. Malformed payload → reject at the boundary with a specific status. Unexpected state → fail loudly, not with a shrugging default. Silent degradation is worse than failure because the caller never learns.

**Red flag:** `env.FOO ?? ''`, `config.value || 'default'`, `if (!x) return []`, `try { ... } catch { return null }` on code paths where null is a legitimate value meaning "missing" instead of "error".

## 9. Before you hardcode a credential, secret, URL, or IP — stop.

Read from env/secrets manager. Sanitize external input at the boundary, not halfway through. Request the narrowest IAM/scope you need. If you're about to embed a value that depends on environment, use config.

**Red flag:** a string constant that starts with `sk-`, `AKIA`, `https://prod.`, or looks like a URL with credentials in the path.

## 10. Before you write logic you can only test by booting the app — extract it.

A function that takes concrete inputs and returns concrete outputs is testable; a function that reads globals, queries a DB, and sends emails is not. Push the effects to the edges, keep the logic pure.

**Red flag:** your only test strategy is "spin up the stack and make a request".

## 11. Before you finish a function that's >50 lines or requires "and" to describe — split it.

Max-50 is not holy writ, but it's a signal. If your function's name is "processOrderAndSendConfirmation" it's two functions. If the body has blank-line-delimited sections, each section is probably its own function.

**Red flag:** scrolling through one function. Blank lines used to demarcate "phases". A docstring that reads like a TODO list.

## 12. Before you write a comment — check if it restates the code.

Comments explain *why* — a non-obvious constraint, a workaround for a specific bug, a hidden invariant, domain context that can't be encoded in a type. If removing the comment wouldn't confuse a future reader, delete it. Never write a comment that paraphrases the next line.

**Red flag:** `// loop over users`, `// check if valid`, `// return the result`. Also: referring to the current task / fix / PR number — that rots the moment the code moves. Comments that note a task or ticket number for reference is okay.

## 13. Before you start coding, read the surrounding files.

Language version, framework idioms, linter config, naming patterns, test conventions. Match what's already there; consistency outranks personal preference. If the project can't be inferred and it materially affects the output, ask.

**Red flag:** your code looks stylistically different from its neighbours (imports order, naming casing, error handling pattern, testing library).

## 14. Before you write "ready", "done", "works", "fixed", "deployed" — produce the evidence.

Run the command that proves it. Include the output — or the exit code, or the concrete state — in your response. If you don't have evidence to hand, label the claim as a prediction: "I believe this should work because X, but I haven't verified."

When the claim is about a deploy-able artifact, the evidence is a literal command sequence the user runs, starting from their current state, ending with a verification whose expected output you describe. "Ready, with three caveats" is not a ready claim; the caveats are the script.

**Red flag:** you're about to type "ready to deploy" without having just run `terraform plan` or `docker run` or equivalent. You're about to write a bulleted list of "things to check" instead of a numbered script.

## 15. Never ship an artifact that requires reader convention to be operational.

If a file, flag, or setting sits in the repo looking done but requires the reader to know a naming convention or recognise an `.example` suffix to actually activate it, you've made the repo unreadable at face value. A reviewer skimming the diff can't tell which files are live and which are inert.

Acceptable alternatives:
- Check in active config guarded by a boolean flag (`count = var.enable_ts ? 1 : 0`, feature flag defaulting to off). Activation is one visible variable flip.
- Put the template in a README code block, not as a standalone file next to real ones.
- Generate the file at install time via a `bin/setup` script that prompts for values.

The `.env.local.example` → `.env.local` pattern is the one narrow exception, and only when step 1 of the setup instructions is literally `cp .env.local.example .env.local`. Don't extend it to `.tf.example`, `.yml.example`, etc.

**Red flag:** you're about to check in a file whose name ends in `.example`, `.template`, `.sample`, `.disabled`, or similar. You're about to comment out code and leave it, intending the reader to uncomment it later.

## 16. Diagnose simplest-first.

When the user reports an unexpected outcome, the first question is always "did the thing run / does the resource exist?" Not "what exotic failure mode could produce this symptom?"

Rung-by-rung:
1. Does the resource exist? (`kubectl get`, `gcloud ... list`, `ls`, `terraform state list`, file existence)
2. Did the intended process run? (CI status, logs, command history, last-modified times)
3. Did it touch the right inputs/outputs? (file contents, image digests, config values)
4. Is its configuration correct? (env vars, flags, permissions)
5. Only now: mechanism-level explanations (caching, digest pinning, race conditions, protocol mismatches)

Work up, not down. The boring precondition is almost always the real failure.

**Red flag:** you're theorising about Cloud Run digest pinning or DNS caching before running `list` to confirm the thing even exists. You're reaching for a complex explanation when a simpler one hasn't been ruled out.

## 17. When you catch yourself about to violate 14–16, stop and rewrite.

Triggers that mean you're shipping a bad handoff:
- "Now just run X" without confirming X works from the user's current state.
- "Fill in real values" without showing the exact format, field by field.
- A "ready" claim followed by three caveats — the caveats are the script; move them inline.
- Jumping to a sophisticated failure theory before checking whether the obvious precondition holds.
- A suggestion that depends on the user noticing a file-name suffix, a commented-out block, or a convention you didn't state.

In each case: rewrite as a numbered script, run the simplest diagnostic yourself, or spell out the convention explicitly as step 1.
