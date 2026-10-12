# Releasing v2.1.29

## Local gate

```bash
npm ci
npm test
npm run check
npm run build
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
node scripts/audit-dependencies.mjs
npm audit --audit-level=high
cargo run --release --bin news-scraper -- collect --date 2026-10-05
npm run tauri build
```

Inspect the JSON contract and Excel workbook. Confirm 88 selected sources, classify any failed source by cause, require no quality alert, schema version 4 and the expected relevance hash. A temporary external outage must be documented and must not be described as a full success.

## Publication gate

1. Merge the validated commit to `main`.
2. Create and push immutable tag `v2.1.29`; do not modify any previous tag or Release, including the published rollback `v2.0.0`.
3. The `Build and release Rust apps` workflow must pass the all-source gate and all four platform builds.
4. Verify the Linux/macOS ZIPs, Windows portable ZIP, Windows Setup EXE, SHA-256 manifest, size manifest and the non-draft GitHub Release.
5. Confirm archives contain the Rust CLI and Tauri app but no `.py`, Python runtime, PyInstaller, openpyxl or Selenium.
6. Confirm the Windows portable ZIP has top-level `各機關新聞整理.exe`, `cli/news-scraper.exe`, and no `START-GUI.cmd`.
7. Run `Release live smoke` for `v2.1.29` and verify all published downloads, including the extracted Windows portable GUI rendered-interface marker.

A tag without a completed workflow, release assets and successful smoke is not a completed release.

## Source-health retry policy

The full 88-source run must satisfy schema, policy-hash, nonempty-news and quality-alert checks. Each initially failed source receives one separate CLI retry before the gate evaluates its final failure class. A recovered parser failure is accepted with both attempts retained; unresolved parser/access/runtime failures block publication. Remaining failures may only be attributed to `source_outage`, `runner_network` or `tls_certificate`, with valid nonzero counts and matching retry evidence.

The default ceiling is **8 final failed sources and 10%**, both enforced. 數位發展部、國發會、國科會 and 經濟部 must recover regardless of that ceiling. The explicit policy lives in `scripts/release-source-gate.mjs`; update its tested defaults deliberately when the release policy changes. `source-health/final-gate.json` records the initial/final failure lists, policy limits and rejection reasons, alongside all original reports and retry evidence. Do not describe an accepted degraded run as all sources successful.

The audit wrapper runs the standard RustSec scan first. Only the known duplicate RUSTSEC-2026-0244 is handled, and only when both gettext packages are absent from Cargo.lock. A temporary database copy removes the duplicate for the unused gettext-sys package; the original database and all applicable advisories remain intact. Any other database error or applicable vulnerability still fails the gate.

If a GitHub Linux runner exceeds the existing failure budget solely because of runner_network errors, first rerun the failed gate and compare a same-week full local run. Manual publication may select source_gate_os=macos-15 to perform the same complete 88-source gate in another hosted environment. Tag-triggered runs retain the Ubuntu default. This option never changes thresholds, skips sources, substitutes local evidence for the hosted gate, or relaxes TLS policies. The four-platform build matrix remains unchanged.

## One-job local source gate

When official sites block both hosted environments but work from the release operator's Mac, manual publication from `main` may select `source_gate_os=self-hosted-macos`. The source job requires the labels `self-hosted`, `macOS`, and `news-source-gate`. It builds the exact workflow commit and executes the same full 88-source collection, one retry for each failed source, and unchanged release gate. The four platform builds and asset publication still run on GitHub-hosted machines. An unresolved culture-ministry verification page still blocks publication even on the local runner.

After merging the reviewed change, checking out its exact `main` commit, and creating the immutable version tag and draft Release:

```bash
gh workflow run build-release.yml --ref main -f version=2.1.31 -f source_gate_os=self-hosted-macos
gh run list --workflow build-release.yml --event workflow_dispatch --limit 5
# Wait for "Gate all 88 live sources" to be queued, then use that run ID:
node scripts/start-release-source-runner.mjs <run-id>
```

The starter verifies the manual event, main branch, workflow path, exact local commit, queued job labels, Chrome and command dependencies before registration. It downloads the official runner and verifies GitHub's SHA-256 checksum, registers an ephemeral runner, and runs one job in the foreground. No background service is installed. Registration credentials are kept out of files and output. Stop with Ctrl-C if necessary; the starter removes only its own remaining registration. Runner work and logs stay under `~/.local/share/taiwan-news-release-runners/`, outside the repository.

Use this only for reviewed release code: this public repository must not send pull-request or other untrusted jobs to the personal Mac. Start it only after the selected release job is queued, and keep the standard PR workflows on hosted runners. See [GitHub's self-hosted runner guidance](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/add-runners).

Every source run writes to a new temporary directory, preventing stale reports or retry evidence from a previous job from passing the gate. The uploaded `release-source-health` artifact includes `execution-context.json` identifying the commit, run, selected runner and operating system, plus the fresh collection and retry evidence. Inspect `final-gate.json`; a runner exiting normally is not evidence that the source gate passed.
