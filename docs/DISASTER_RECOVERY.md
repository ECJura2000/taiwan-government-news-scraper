# Disaster recovery

1. Preserve generated Excel and JSON reports outside the checkout.
2. Re-clone the repository and verify the desired signed tag or commit.
3. Run `cargo test --workspace --all-targets`, frontend checks and `news-scraper list-sources`.
4. Build the release CLI and run a deterministic single-source smoke before all sources.
5. If v2.1.0 cannot pass source or artifact gates, use the unchanged v2.0.0 GitHub Release as rollback; do not overwrite either release.
