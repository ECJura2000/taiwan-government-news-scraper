# Disaster recovery

1. Preserve generated Excel and JSON reports outside the checkout.
2. Re-clone the repository and verify the desired signed tag or commit.
3. Run `cargo test --workspace --all-targets`, frontend checks and `news-scraper list-sources`.
4. Build the release CLI and run a deterministic single-source smoke before all sources.
5. The signed v2.1.0, v2.1.1 and v2.1.2 candidates remain unpublished. v2.1.3 and v2.1.4 remain immutable historical releases; v2.1.4 should not be recommended because its Svelte GUI does not mount. The unchanged v2.0.0 GitHub Release remains the rollback build while publishing v2.1.9.
