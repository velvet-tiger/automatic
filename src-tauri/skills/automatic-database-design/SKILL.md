---
name: automatic-database-design
description: Schema design conventions, normalisation, indexing, and migration practices. Use when designing or reviewing a relational database schema.
authors:
  - Automatic
---

# Database Design

A schema is hard to change after data is in it. Design deliberately. The cost of a poor schema compounds with every migration, query, and application feature built on top of it.

## Principles

**Model the domain, not the UI.** Schema design should reflect the real-world entities and their relationships, not the shape of a form or API response. Those can change; the underlying domain is more stable.

**Normalise to remove redundancy, denormalise to meet performance requirements.** Start normalised — store each fact once. Denormalise only when profiling identifies a specific, measured need.

**Constraints belong in the database.** Uniqueness, nullability, foreign keys, and check constraints enforced by the database are reliable. Constraints enforced only by application code can be bypassed. Use both.

---

## Naming

- Use `snake_case` for tables and columns
- Use plural nouns for tables: `users`, `orders`, `line_items`
- Use singular nouns for join tables that represent a named concept: `user_role` (not `users_roles`)
- Foreign keys: `{referenced_table_singular}_id` — e.g. `user_id`, `order_id`
- Avoid reserved words and generic names: `data`, `value`, `info`, `type` are ambiguous and cause quoting issues
- Boolean columns: name with an adjective — `is_active`, `has_verified_email`, not `active`, `verified`
- Timestamp columns: use `_at` suffix — `created_at`, `published_at`, `deleted_at`

---

## Primary Keys

- Use surrogate keys (generated identifiers) rather than natural keys for primary keys
- Use UUIDs (v4 or v7) for distributed systems or when IDs are exposed externally — they do not leak record counts or creation order
- Use auto-increment integers for internal tables where ID exposure is not a concern — they are smaller, index more efficiently, and sort naturally
- Never use mutable data as a primary key

---

## Relationships

**One-to-many** — foreign key on the "many" side.

**Many-to-many** — join table with foreign keys to both sides. Give the join table a meaningful name if the relationship has its own attributes.

**One-to-one** — use when a table has optional or rarely-accessed data that would otherwise bloat the main table. Foreign key on the dependent side.

Always define foreign key constraints. They enforce referential integrity at the database level and make the schema self-documenting.

---

## Nullability

A nullable column means "this value may be unknown or inapplicable". Be deliberate:

- Make columns `NOT NULL` by default
- Add `NULL` only when the absence of a value is a valid, meaningful state — not as a shortcut
- A nullable foreign key means "this row may not be associated with a parent" — is that correct?
- Prefer empty string `''` to `NULL` for optional text fields when the distinction between "not provided" and "empty" matters

---

## Indexes

Indexes speed up reads and slow down writes. Add them intentionally.

**Index automatically:**
- Primary keys (automatic in most databases)
- Foreign key columns — without an index, joins and cascade operations scan the table
- Columns used in `WHERE`, `ORDER BY`, or `GROUP BY` on large tables

**Index carefully:**
- Composite indexes are ordered — `(a, b)` helps queries filtering by `a` or `a + b`, not `b` alone
- Partial indexes on a subset of rows can be highly effective (e.g. `WHERE deleted_at IS NULL`)
- Unique indexes enforce uniqueness as a constraint, not just for performance

**Do not index:**
- Every column — index bloat degrades write performance
- Low-cardinality columns (e.g. boolean, status with 3 values) without a partial index

Use `EXPLAIN` to verify that queries use the indexes you expect.

---

## Migrations

- Every schema change is a migration — never modify the schema directly in production
- Migrations must be: reversible (include `down` migrations), idempotent where possible, and tested against a copy of the production schema
- Avoid long-lock migrations on large tables during peak traffic (e.g. adding a non-null column with a default to a 100M-row table)
- Separate data migrations from schema migrations — they have different risk profiles

---

## Soft Deletes

Adding a `deleted_at` timestamp instead of hard-deleting records preserves history but has costs:

- Every query must filter `WHERE deleted_at IS NULL` — easy to forget
- Unique constraints must account for soft-deleted rows
- Tables grow without bound

Use soft deletes deliberately. Consider an audit log or separate archive table as alternatives for the history use case.

---

## Timestamps

Include `created_at` and `updated_at` on every table. They are invaluable for debugging and auditing.

- Store as UTC; convert to local time in the application layer
- Use database-level defaults: `DEFAULT now()` for `created_at`; trigger or application update for `updated_at`
- Use timezone-aware types (`TIMESTAMPTZ` in PostgreSQL) rather than naive timestamps
