# ADR 0001: Thread Pool And Selective Retry

## Status

Accepted

## Decision

Use a thread pool because the 72 source scrapers are primarily blocking network I/O and
many existing parsers use synchronous libraries. Use a heap to start risky/slow sources
first, but keep final Excel ordering deterministic.

Only download/network failures are retried. Parse, validation, and storage failures are
recorded immediately because repeating the same input will not repair them.

## Consequences

The design reuses existing scrapers and reduces wall-clock time without requiring every
parser to become asynchronous. Worker counts must be benchmarked to avoid excessive
connections and diminishing returns.
