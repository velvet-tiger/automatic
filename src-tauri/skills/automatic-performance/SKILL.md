---
name: automatic-performance
description: Data-driven approach to identifying and resolving performance bottlenecks. Use when investigating slow queries, high latency, or memory growth.
authors:
  - Automatic
---

# Performance Profiling

Optimise only what you measure. Intuition about performance bottlenecks is frequently wrong. Every performance change must be preceded by data that identifies the actual constraint.

## The Process

### 1. Establish a baseline
Before changing anything, measure the current performance under realistic conditions.

- Use production-representative data volumes and request patterns
- Record the metric you care about: p50, p95, p99 latency; throughput; memory usage; CPU time
- Run measurements multiple times and take medians — single measurements are noisy

### 2. Profile, do not guess
Use a profiler to identify where time is actually spent. The results will surprise you.

**CPU profiling** reveals which functions consume the most CPU time. Look for:
- Hot functions called far more often than expected
- Algorithmic complexity problems (O(n²) loops, exponential recursion)
- Unnecessary serialisation or deserialisation in hot paths

**Memory profiling** reveals allocation patterns. Look for:
- Objects allocated and immediately discarded in tight loops
- Memory that grows without bound (leaks)
- Large allocations that could be pooled or reused

**I/O and query profiling** — for most web applications, the bottleneck is database or network, not CPU. Look for:
- N+1 query patterns (one query per item in a loop)
- Missing indexes on frequently filtered or sorted columns
- Unbounded queries (no `LIMIT`, fetching far more data than needed)
- Synchronous blocking calls that could be concurrent

### 3. Fix one thing at a time
Change one thing, re-measure, and compare against the baseline. If you change multiple things simultaneously, you cannot attribute the improvement.

### 4. Verify the improvement
The improvement must be measurable in the same metric you established in step 1. If you cannot measure it, the optimisation did not work.

---

## Common Bottlenecks

### N+1 queries
Symptom: query count scales linearly with the number of records processed.

```
// N+1: one query to get orders, then one query per order to get the user
orders = db.query("SELECT * FROM orders")
for order in orders:
    user = db.query("SELECT * FROM users WHERE id = ?", order.user_id)
```

Fix: fetch related data in a single join, or use batch loading.

### Missing database indexes
Symptom: full table scans on large tables; slow queries on filtered or sorted columns.

Diagnosis: run `EXPLAIN` on slow queries; look for sequential scans on large tables.

Fix: add indexes on columns used in `WHERE`, `ORDER BY`, and `JOIN` conditions. Note that indexes have a write cost — add them deliberately.

### Synchronous I/O in hot paths
Symptom: high latency, low CPU utilisation — the process is waiting.

Fix: use async I/O; process independent requests concurrently; cache results of expensive I/O.

### Unnecessary serialisation
Symptom: high CPU time in JSON encode/decode, XML parsing, or protocol buffer serialisation.

Fix: cache serialised representations; reduce the size of serialised payloads; avoid serialising in tight loops.

### Unbounded memory growth
Symptom: memory usage grows linearly with time or request count without stabilising.

Fix: identify what is accumulating and why it is not being released. Common causes: event listeners not removed, caches without eviction policies, accumulating state in long-lived processes.

---

## Caching

Cache at the right level:
- **In-process** — fastest; lost on restart; not shared across instances
- **Distributed** (Redis, Memcached) — shared across instances; adds network latency
- **HTTP** (CDN, browser) — most scalable; only for public, cacheable responses

Cache invalidation must be explicit. Stale data is a correctness problem, not just a performance one. Define TTLs deliberately; shorter TTLs are safer.

Only cache what you have measured to be worth caching. Caches add complexity; they are not free.

---

## What Not to Do

- **Do not optimise prematurely** — write correct, clear code first; profile when there is a measured problem
- **Do not micro-optimise** — shaving 10ns from a function called 100 times/second saves 1µs/s; it is not worth the complexity
- **Do not cache without measuring** — if the operation is not a bottleneck, the cache buys nothing
- **Do not guess** — form a hypothesis, measure, act on evidence
