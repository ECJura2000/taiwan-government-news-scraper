# ADR 0001: bounded async scheduling and route fallback

## Status

Superseded by the Rust v2.1.0 implementation.

## Decision

Use Tokio and bounded unordered buffering for source concurrency. Each source executes its declarative official routes in priority order or aggregates routes when the catalog requires it. Final Excel ordering is deterministic.

Network, browser and parser outcomes are retained as route attempts. A successful fallback completes the source but remains visible as unstable. Cancellation uses a shared atomic flag and does not write partial artifacts.
