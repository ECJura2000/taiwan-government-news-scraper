# Guided desktop UI design QA — 2026-10-10

final result: passed

## Visual truth and evidence

- Selected direction: second displayed design, with the user's requested 標楷體 / Times New Roman revision.
- Source visual truth: `/Users/ericchi/.codex/generated_images/01a10b8c-5ea1-78b3-9b4a-d65926949240/exec-817ef17c-2370-4479-b819-d180351b6051.png`.
- Source pixels: 1487 × 1058; normalized to 1440 × 1024, preserving aspect ratio to within one pixel.
- Browser implementation: `http://127.0.0.1:1420/?preview=1`, 1440 × 1024 CSS pixels; 1440 × 1024 screenshot at 1× density: `/tmp/tw-ui-light-final.png`.
- Full-view comparison, both artifacts in the same image: `/tmp/tw-ui-comparison-final.png`.
- Focused typography, date selector and settings-row comparison: `/tmp/tw-ui-comparison-details.png`.
- Native packaged application: `target/release/bundle/macos/各機關新聞整理.app`; screenshot `/tmp/tw-ui-native-final.png`, 2880 × 1696 pixels at 2× density. Window 1440 × 848 points; content height approximately 816 points after title bar. This deliberately exercises the compact height breakpoint, rather than stretching the 1024-high source to this height.
- Additional responsive evidence: `/tmp/tw-ui-narrow-dark-200.png`, 600 CSS pixels wide, dark theme, 200% text size, vertically scrolling full-page capture.
- State: ready to confirm scope, previous complete reporting week, all approved sources selected. Preview uses public catalog/default topic data. Native application successfully loads the real policy and 88 sources; its collection and folder-selection buttons are enabled.

## Findings and comparison history

1. [P2, fixed] Original body-size override retained 14px; mock hierarchy and Kai readability were weaker than intended. Increased base size to 16px, increased desktop title hierarchy, and retained 100%–200% scaling. Confirmed real BiauKaiTC / 標楷體-繁 family names from the Apple font's name table; the older BiauKai name alone did not resolve this system's font. Final screenshots show Kai Chinese text and serif Latin/digits.
2. [P2, fixed] Early desktop capture placed the primary action partly below the initial viewport. Adjusted workflow gaps and row padding; the final 1440 × 1024 browser action is fully visible.
3. [P2, fixed] The native macOS window is shorter than the source mock. Added a `min-width:761px` / `max-height:900px` compact variant, rebuilt and reopened the packaged app. `/tmp/tw-ui-native-final.png` shows header, scope rows, advanced settings and enabled primary collection button together, without scrolling.
4. [P2, fixed] At 200% text size, the native select's date label truncates. The entire date range is also shown in a wrapping text line immediately below it. The 600-wide dark capture shows the complete range, all settings and reachable primary action without horizontal overflow.

No actionable P0/P1/P2 findings remain in the implemented scope.

## Required fidelity surfaces

- **Fonts / typography:** English and numbers use Times New Roman first, CJK falls back to Windows DFKai-SB or macOS BiauKaiTC / 標楷體-繁. Font files are not included in the repository or application bundle. The source mock's illustrated brush weight differs slightly from the actual licensed system font; the user's named font takes precedence. Hierarchy, readable line heights and larger text reflow verified.
- **Spacing / layout:** Same three-step progression, left labels, aligned editable scope rows, collapsed advanced options and bottom-right primary action. Additional preview notice is development-only. Compact native height and narrow / enlarged layouts intentionally reflow.
- **Colors / tokens:** Light navy typography, white/light neutral surface and teal primary action; dark tokens use light text and teal accent. Disabled preview controls are intentionally gray; the native primary action is teal and enabled. Visible focus treatment retained.
- **Assets / icons:** No custom illustrations or raster assets are needed. Step numbers and native form controls are semantic UI elements. Optional decorative arrows from the mock were omitted instead of approximated with fake icons.
- **Copy / content:** Traditional Chinese preserved. Actual default profile has 10 topics, versus the mock's illustrative 5; approved catalog has 88 sources. No fabricated historical report, prior successful collection or preview success result. Date range is calculated in Asia/Taipei, and CLI date defaults remain unchanged.

## Interaction verification

- Previous complete week, custom dates and return-to-default control tested.
- Reversed custom dates show the error; incomplete/custom invalid ranges disable collection. Unit tests cover Monday/weekend, Taiwan UTC boundary and cross-year weeks.
- Source settings open/collapse; search for 高速公路 yields 高速公路局.
- Topic settings open/collapse; existing rule editor opens, cancel closes it and returns focus to the originating rule button.
- Light/dark theme and 100%/200% text size tested; 600-wide / 200% dark view has no horizontal overflow.
- Browser error console checked: no errors in the explicit development preview.
- Packaged native application opened and loads real 10-topic policy / 88 sources, with enabled collection and storage controls.
- No real news collection or native file write was initiated solely for UI verification. Rust CLI/output regression fixtures test the existing collection and report contract.

## Validation

- `npm run check`: zero errors / warnings.
- `npm run build` and native Mac application bundle: passed after final layout changes.
- `npm test`: 16 frontend tests and 6 release-gate tests passed.
- Rust format and all-features clippy: passed.
- `cargo test --workspace --all-targets`: 130 passed, 1 existing live-feed test ignored. Local fixture-server tests required running outside the port-binding sandbox; rerun passed.
- CLI approved source catalog: 88 sources.
- `git diff --check`: passed.

## Follow-up polish / test gaps

- P3: Actual system Kai stroke weights and native macOS select arrows differ from Image Gen artwork; named system fonts and native controls are intentional.
- Windows and Linux installed-app visual checks were not performed on this Mac. Those systems must have the requested fonts installed; otherwise the declared serif fallback applies.
- Full live collection, cancellation during live collection and post-run native file opening were not executed in this UI task. Existing Rust contract tests remain passing.
