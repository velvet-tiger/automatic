---
name: automatic-api-design
description: REST API design conventions, error shapes, versioning, and pagination patterns. Use when designing or reviewing any HTTP API.
authors:
  - Automatic
---

# API Design

A good API is easy to use correctly and hard to use incorrectly. Design for the consumer, not the implementation. An API is a contract — once published, breaking it is costly.

## Principles

**Consistency above all.** Inconsistency is the primary source of API usability bugs. Naming conventions, error shapes, pagination patterns, and status codes must be uniform across all endpoints.

**Design for the consumer's task.** APIs should match the operations a client actually performs, not the shape of your internal data model. If a client always fetches user + profile + settings together, consider whether that should be one call.

**Make invalid states unrepresentable.** If a field combination is invalid, the API should not accept it. Validation errors at the boundary are far cheaper than invalid state in the system.

---

## REST Conventions

### Resources and URLs
- URLs identify resources (nouns), not actions: `/orders/123`, not `/getOrder?id=123`
- Use plural nouns for collections: `/users`, `/orders`
- Nest resources to express ownership: `/users/42/orders`
- Keep nesting shallow — avoid more than two levels deep
- Use kebab-case for multi-word segments: `/payment-methods`

### HTTP methods
| Method | Purpose | Idempotent | Safe |
|--------|---------|-----------|------|
| GET | Read resource | Yes | Yes |
| POST | Create resource or trigger action | No | No |
| PUT | Replace resource entirely | Yes | No |
| PATCH | Partial update | No | No |
| DELETE | Remove resource | Yes | No |

### Status codes
- `200 OK` — successful GET, PATCH, or DELETE
- `201 Created` — successful POST that created a resource; include `Location` header
- `204 No Content` — successful operation with no body (e.g. DELETE)
- `400 Bad Request` — client sent invalid data
- `401 Unauthorized` — not authenticated
- `403 Forbidden` — authenticated but not permitted
- `404 Not Found` — resource does not exist
- `409 Conflict` — state conflict (e.g. duplicate, version mismatch)
- `422 Unprocessable Entity` — syntactically valid but semantically invalid
- `429 Too Many Requests` — rate limited; include `Retry-After` header
- `500 Internal Server Error` — unexpected server failure

---

## Error Responses

Return errors in a consistent, machine-readable shape:

```json
{
  "error": {
    "code": "VALIDATION_FAILED",
    "message": "The request could not be processed.",
    "details": [
      { "field": "email", "message": "Must be a valid email address" }
    ]
  }
}
```

- `code` — a stable, uppercase string identifier (not a number) clients can switch on
- `message` — human-readable; safe to display
- `details` — optional array of field-level errors for validation failures
- Never return internal stack traces, file paths, or system information

---

## Versioning

Version APIs from day one. You will need to change them.

- Include the major version in the URL: `/api/v1/users`
- Increment the major version only on breaking changes
- Maintain the previous major version for a documented deprecation window
- Non-breaking additions (new optional fields, new endpoints) do not require a version bump

**Breaking changes include:** removing fields, changing field types, changing status codes for existing cases, changing required/optional status of fields, removing endpoints.

---

## Pagination

For any endpoint that returns a list, always paginate.

Prefer cursor-based pagination for large or frequently updated datasets:

```json
{
  "data": [...],
  "pagination": {
    "next_cursor": "eyJpZCI6MTAwfQ==",
    "has_more": true
  }
}
```

Offset-based pagination (`?page=2&limit=20`) is simpler but inconsistent under concurrent writes — use it only for stable, infrequently updated data.

Always document the maximum page size and enforce it.

---

## Request and Response Design

- Use consistent field naming — choose `camelCase` or `snake_case` and use it everywhere
- Always return the full representation of a created or updated resource in the response body — do not make clients issue a follow-up GET
- Use ISO 8601 for all timestamps: `2025-03-02T14:30:00Z`
- Use strings for IDs if they may exceed 53-bit integer precision (e.g. database IDs in languages where JSON numbers are 64-bit floats)
- Make optional fields explicit — return `null` rather than omitting the field

---

## Before You Ship

- [ ] Every endpoint is documented with request/response examples
- [ ] All error codes are documented and consistent
- [ ] Authentication requirements are documented for every endpoint
- [ ] Rate limits are defined and communicated via response headers
- [ ] Breaking vs. non-breaking changes are distinguished in the changelog
- [ ] The API is tested from the consumer's perspective, not just the implementation's
