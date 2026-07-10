# Adaptive Overlay Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fixed overlay breakpoints and unbounded text painting with content-aware columns, font-measured keycaps, bounded action text, and ordered balanced group placement.

**Architecture:** Keep the existing rendering and test structure in `src/app.rs`. Add small pure layout contracts beside the existing overlay helpers, use egui's active fonts for measurement, use `TextWrapping` and `Galley::elided` for one- and two-row text, and integrate the results into `show_shortcut_columns` without changing storage or app-sheet schemas.

**Tech Stack:** Rust 2024, eframe/egui 0.34.1, existing in-module Rust tests, Cargo.

## Global Constraints

- Source of truth: `docs/superpowers/specs/2026-07-11-adaptive-overlay-layout-design.md`.
- Keep the current warm card, typography roles, keycap colors, radius, vertical scroll area, and maximum of four columns.
- Do not add a manual column-count setting or any new persisted setting key.
- Use exact existing storage and app-sheet contracts; no fallback keys or legacy aliases.
- Multi-column actions use one row; one-column actions use at most two rows.
- Every column, action, and combo run receives an explicit clip rectangle.
- The configured `row_height` remains the minimum row height; a two-line galley may expand beyond it.
- Preserve app-sheet group order through contiguous balanced partitions.
- Keep all implementation and tests in `src/app.rs`; the repository rule against premature extraction takes precedence over creating a new module.
- Follow red-green-refactor for every production behavior.
- This plan intentionally contains no implementation code, per repository documentation rules.

## Canonical Long-Content Fixture

Use this exact in-memory fixture in every calculation, renderer, and manual comparison. Tests must not read the user's custom app-sheet files.

**Resolved style and geometry:** window sizes `1755×873` and `920×560`; `20px` card padding; exact inner content widths `1715px` and `880px`; `12px` action font; `13px` group-heading font; `9.5px` keycap font; `18px` configured row height; `16px` keycap height; `3px` keycap gap; `170px` configured combo rail; zero action gap; `-0.75px` action optical offset; `12px` row item spacing; `12px` column gap; `6px` heading gap; and `16px` inter-group gap. The layout calculator and renderer test UI receive the inner content width directly; they never subtract card padding again.

| Group order | Combo | Action |
|---|---|---|
| 탐색 | `Ctrl+Shift+P` | 워크스페이스 전체 명령 팔레트를 열고 실행 가능한 작업을 빠르게 검색합니다 |
| 탐색 | `Ctrl+P` | 파일 이름과 상대 경로를 기준으로 현재 프로젝트의 문서를 즉시 찾습니다 |
| 탐색 | `Ctrl+Shift+O` | 현재 파일의 심볼 목록을 열어 함수와 타입 정의로 이동합니다 |
| 탐색 | `Alt+Left` | 이전에 확인한 편집 위치로 돌아가 탐색 흐름을 계속 이어갑니다 |
| 편집 | `Ctrl+Shift+K` | 현재 선택 영역과 연결된 줄을 삭제하고 다음 편집 위치를 유지합니다 |
| 실행 | `Ctrl+Shift+B` | 현재 작업의 기본 빌드 태스크를 실행하고 출력 패널에서 결과를 확인합니다 |
| 실행 | `Ctrl+F5` | 디버거 연결 없이 현재 프로젝트를 실행하고 종료 상태를 확인합니다 |
| 실행 | `Ctrl+Shift+Alt+Super+PageDown` | 선택한 자동화 세션의 마지막 실행 단계와 상세 이벤트 로그로 이동합니다 |
| 세션 | `Ctrl+Alt+N` | 새 작업 세션을 열고 현재 워크스페이스 컨텍스트를 그대로 이어받습니다 |
| 세션 | `Ctrl+Alt+W` | 활성 세션을 안전하게 닫고 아직 저장하지 않은 변경을 먼저 확인합니다 |
| 검토 | `Ctrl+Shift+G` | 소스 제어 변경 목록을 열고 파일별 diff와 스테이징 상태를 검토합니다 |
| 검토 | `F8` | 현재 파일에서 다음 진단 오류 또는 경고 위치로 이동합니다 |
| 검토 | `Shift+F8` | 현재 파일에서 이전 진단 오류 또는 경고 위치로 이동합니다 |
| 검토 | `Ctrl+K+Ctrl+X` | `C:\workspace\packages\adaptive_overlay\generated\artifacts\verification\report.json` |
| 도움말 | `F1` | 현재 화면에서 사용할 수 있는 명령과 관련 도움말 문서를 함께 엽니다 |
| 도움말 | `Ctrl+K+Ctrl+S` | 단축키 편집기를 열어 현재 키 조합의 충돌과 적용 범위를 확인합니다 |

The group entry counts are `[4, 1, 3, 2, 4, 2]`. At `1715px` inner width, the expected action target is `320px`, the expected column count is three, and the expected ordered ranges are `[0..2, 2..4, 4..6]`. At `880px` inner width, the same fixture uses one column. The long path action and the oversized combo are the canonical containment and tooltip probes.

---

### Task 1: Pure width targets and adaptive column selection

**Files:**
- Modify: `src/app.rs:1193-1308`
- Modify: `src/app.rs:1727-1791`
- Test: `src/app.rs:2092-2586`

**Interfaces:**
- Consumes: measured action widths, measured combo widths, configured `combo_width`, `action_gap`, row item spacing, exact inner content width, column gap, and visible group count.
- Produces: `target_action_width(&[f32]) -> f32` using nearest-rank P90 clamped to `180..=320`; input measurements must be finite and empty input resolves to `180`.
- Produces: `target_combo_width(&[f32], configured_width: f32) -> f32` using nearest-rank P90, the configured minimum, and the `220` cap; input measurements and configured width must be finite.
- Produces: `OverlayLayoutMetrics` containing target widths, selected column count, final column width, and final action width.
- Produces: `calculate_overlay_layout(available_content_width, group_count, target_combo_width, target_action_width, row_item_spacing, action_gap, column_gap) -> OverlayLayoutMetrics`, selecting `1..=min(4, group_count)` columns while retaining the target action width. `available_content_width` is the current `ui.available_width()` inside the padded card; this helper never consumes window width or card padding.

- [ ] **Step 1: Add the failing action-target percentile test**

  Add `action_target_width_uses_nearest_rank_percentile_and_clamps`. Cover an unsorted width list, the nearest-rank P90 result, the `180` minimum, the `320` maximum, and the empty-input result.

- [ ] **Step 2: Run the percentile test and confirm RED**

  Run: `cargo test app::tests::action_target_width_uses_nearest_rank_percentile_and_clamps -- --exact`

  Expected: compilation fails because `target_action_width` does not exist.

- [ ] **Step 3: Implement the minimum percentile and clamp behavior**

  Add only the constants and target-width helper required by the test. Require finite measurements, fail clearly if that precondition is broken, sort deterministically with total ordering, and use the normalized lower bound for an empty input. Do not silently discard an invalid measurement.

- [ ] **Step 4: Run the percentile test and confirm GREEN**

  Run: `cargo test app::tests::action_target_width_uses_nearest_rank_percentile_and_clamps -- --exact`

  Expected: one test passes.

- [ ] **Step 5: Add failing combo-target and column-selection tests**

  Add these tests:

  - `combo_target_width_honors_configured_minimum_and_cap`
  - `adaptive_columns_never_exceed_group_count`
  - `one_group_layout_always_uses_one_column`
  - `long_description_layout_uses_three_columns_at_current_width`
  - `short_wide_layout_uses_four_columns`
  - `adding_column_preserves_target_action_width`
  - `column_transition_keeps_target_action_width`
  - `column_count_is_monotonic_across_resize_sequence`
  - `maximum_style_values_reduce_columns_before_action_width_turns_negative`

  Use the canonical fixture's `1715px` inner content width directly. Test row item spacing and column gap as separate inputs even when both resolve to `12px`.

- [ ] **Step 6: Run the adaptive layout tests and confirm RED**

  Run: `cargo test app::tests::`

  Expected: compilation fails because the combo target and layout calculator do not exist.

- [ ] **Step 7: Implement the minimum adaptive layout calculator**

  Select the greatest legal column count whose computed column width retains the requested action target. For each candidate, subtract column gaps only between columns; derive final action width by subtracting the combo rail, one row item spacing, and configured action gap from the equal column width. Clamp to the group count and four-column maximum. When the minimum useful width does not fit, return one column with a non-negative final action width and let the bounded overflow policy handle the remainder. A group count of zero remains outside this helper because the existing empty-state path returns before column rendering.

- [ ] **Step 8: Run Task 1 tests and the existing app test module**

  Run: `cargo test app::tests::`

  Expected: all app tests pass with no warnings.

- [ ] **Step 9: Commit Task 1**

  Stage only `src/app.rs` and commit with message `feat: 콘텐츠 기반 오버레이 단 수 계산`.

---

### Task 2: Font-backed keycap and content measurement

**Files:**
- Modify: `src/app.rs:1292-1417`
- Modify: `src/app.rs:1727-1791`
- Test: `src/app.rs:2363-2411`

**Interfaces:**
- Consumes: an egui context initialized through the existing `install_korean_font`, `egui::Ui`, `ResolvedOverlayStyle`, shortcut combo strings, and action strings.
- Produces: `measure_keycap_label_width(...) -> f32` from the active monospace font plus the current total horizontal badge padding and `16px` minimum.
- Produces: `measure_combo_run_width(...) -> f32` from measured keycaps plus configured keycap gaps.
- Produces: `measure_action_width(...) -> f32` from the active proportional action font.
- Produces: `overflow_tooltip_text(full_text, overflowed) -> Option<&str>`, shared by combo and action responses, returning the exact original text only when overflow occurred.
- Changes: `keycap_combo_start_x` consumes a measured total run width rather than recomputing a character-count estimate.
- Changes: `show_keycap_combo` uses measured badge widths, clips both badge backgrounds and glyphs to the combo rectangle, preserves the rightmost final key, and reports whether clipping occurred.

- [ ] **Step 1: Add failing font-measurement tests**

  Add these egui-backed tests using `egui::Context::default()`, the production `install_korean_font`, and `Context::run` rather than the empty-font `egui::__run_test_ui` helper:

  - `measurement_context_uses_installed_korean_font`
  - `keycap_width_tracks_resolved_font_metrics`
  - `combo_run_width_includes_configured_keycap_gap`
  - `action_width_uses_resolved_proportional_font`

  Confirm the production Korean font installation succeeds, compare one Korean glyph with a longer Korean label, compare default and maximum font sizes, and verify that a multi-part combo includes exactly the configured number of gaps.

- [ ] **Step 2: Run the measurement tests and confirm RED**

  Run: `cargo test app::tests::`

  Expected: compilation fails because the measurement helpers do not exist.

- [ ] **Step 3: Implement font-backed measurement only**

  Use the active production egui fonts and the same font IDs used by rendering. Preserve the current total badge padding and minimum badge width. Remove the character-count width heuristic only after the new tests are red; do not let tests silently fall back to egui's empty font set.

- [ ] **Step 4: Run the measurement tests and confirm GREEN**

  Run: `cargo test app::tests::`

  Expected: all three tests pass.

- [ ] **Step 5: Add the failing combo containment test**

  Add `oversized_combo_is_clipped_from_the_left_and_keeps_right_edge` and `clipped_combo_requests_full_text_tooltip`. Use a command-like combo whose measured run exceeds a `220px` rail. Verify the reported clipping state, right-edge alignment, and exact original combo exposed to the tooltip response.

- [ ] **Step 6: Run the combo containment test and confirm RED**

  Run: `cargo test app::tests::oversized_combo_is_clipped_from_the_left_and_keeps_right_edge -- --exact`

  Expected: the test fails because combo rendering does not report clipping or use a rail-specific clip.

- [ ] **Step 7: Implement clipped measured keycap rendering**

  Paint through `Painter::with_clip_rect(combo_rect)`. Keep the run right-aligned, return the clipping state, and use the shared overflow-tooltip contract on the existing combo allocation response so the exact full combo appears only when clipping occurs.

- [ ] **Step 8: Run Task 2 tests and the existing right-edge regression**

  Run: `cargo test app::tests::`

  Expected: all matching tests pass.

- [ ] **Step 9: Commit Task 2**

  Stage only `src/app.rs` and commit with message `feat: 키캡 폭을 실제 글리프로 계산`.

---

### Task 3: Bounded action layout, ellipsis, and tooltip

**Files:**
- Modify: `src/app.rs:1300-1432`
- Modify: `src/app.rs:1727-1791`
- Test: `src/app.rs:2388-2411`

**Interfaces:**
- Consumes: action text, final action width, maximum row count, action font, action color, configured `row_height`, keycap height, and configured vertical optical offset.
- Produces: `ShortcutActionLayout` containing the egui galley, resolved row height, vertically shifted galley rectangle, and `elided` state. Resolved row height is at least configured `row_height`, keycap height, and galley height plus twice the absolute optical offset.
- Produces: `layout_shortcut_action(...) -> ShortcutActionLayout` using `TextWrapping.max_width`, `max_rows`, `break_anywhere`, and the default ellipsis character.
- Changes: the existing action renderer paints the prepared galley through `Painter::with_clip_rect(action_rect)` without extracting a second one-use paint helper.
- Changes: the action allocation response uses the shared overflow-tooltip contract and receives the exact complete action only when `Galley::elided` is true.

- [ ] **Step 1: Add the failing multi-column action tests**

  Add these tests:

  - `multi_column_action_uses_one_row_and_elides`
  - `unbroken_action_stays_inside_available_width`
  - `non_elided_action_does_not_request_tooltip`
  - `elided_action_requests_full_text_tooltip`
  - `action_row_contains_positive_and_negative_optical_offsets`

  Use Korean text, mixed Korean/English text, a whitespace-free path-like token, and both normalized optical-offset extremes (`-6px` and `+6px`). Assert that the shifted galley rectangle remains inside the action rectangle in each case.

- [ ] **Step 2: Run the multi-column tests and confirm RED**

  Run: `cargo test app::tests::`

  Expected: compilation fails because `ShortcutActionLayout` and the layout helper do not exist.

- [ ] **Step 3: Implement one-row bounded action layout**

  Build a `LayoutJob` with `max_rows = 1`, the exact action width, and break-anywhere enabled. Derive tooltip need from `Galley::elided`. Keep the existing action color and font size, and include the absolute optical offset when resolving row height so vertical clipping cannot cut a shifted galley.

- [ ] **Step 4: Run the multi-column tests and confirm GREEN**

  Run: `cargo test app::tests::`

  Expected: all matching tests pass.

- [ ] **Step 5: Add the failing single-column two-row tests**

  Add these tests:

  - `single_column_action_uses_at_most_two_rows`
  - `single_column_action_height_expands_for_second_row`
  - `single_column_row_height_honors_configured_minimum`

  Verify that a long action uses two rows, reports elision beyond the second row, and produces a row height at least as large as the configured minimum, keycap, and shifted galley bounds.

- [ ] **Step 6: Run the two-row tests and confirm RED**

  Run: `cargo test app::tests::single_column_action`

  Expected: tests fail because the layout currently supports one unbounded painted line only.

- [ ] **Step 7: Implement two-row single-column layout and clipped painting**

  Set `max_rows = 2` only when the selected column count is one. Allocate the row from the maximum of configured `row_height`, keycap height, and galley height plus twice the absolute optical offset; vertically align the galley with the keycap, paint through the action clip, and attach the exact full tooltip when elided.

- [ ] **Step 8: Run Task 3 tests and action-position regressions**

  Run: `cargo test app::tests::`

  Expected: all matching tests pass.

- [ ] **Step 9: Commit Task 3**

  Stage only `src/app.rs` and commit with message `fix: 긴 단축키 설명을 단 안에 제한`.

---

### Task 4: Ordered balanced groups and full renderer integration

**Files:**
- Modify: `src/app.rs:1727-1814`
- Test: `src/app.rs:2092-2586`

**Interfaces:**
- Consumes: ordered groups, measured heading height, heading gap, row heights after the text policy, inter-group gap, and selected column count.
- Defines: one group block height as heading galley height plus heading gap plus its row heights. It excludes inter-group gap. A candidate column range adds inter-group gap only between its groups, never after the last group.
- Produces: `partition_group_ranges(group_block_heights, inter_group_gap, column_count) -> Vec<Range<usize>>`.
- Produces: contiguous, exhaustive, deterministic group ranges minimizing the tallest column.
- Produces: `PreparedShortcutOverlay` containing layout metrics, measured group block heights, ordered ranges, measured combo runs, and prepared action galleys.
- Produces: `ShortcutRenderReport` containing the consumed metrics and ranges plus final column, combo, and action rectangles, overflow flags, and response IDs used by renderer tests.
- Changes: `show_shortcut_columns(...) -> ShortcutRenderReport` prepares and consumes the new layout instead of fixed breakpoints and modulo assignment. The normal application call discards the report; renderer-level tests use it as the observable integration seam.

- [ ] **Step 1: Add the failing ordered-partition tests**

  Add these tests:

  - `ordered_partition_preserves_every_group_once`
  - `ordered_partition_matches_bruteforce_optimum_for_small_inputs`
  - `ordered_partition_uses_lexicographically_earliest_boundaries_on_ties`
  - `ordered_partition_excludes_trailing_inter_group_gap`

  Enumerate every legal contiguous boundary for several small arrays, including `[100, 20, 20, 20]`, and compare the helper's maximum range height with the brute-force optimum. For the exact tie `[1, 1, 1]`, zero gap, and two columns, require `[0..1, 1..3]`. Separately prove that a two-group range includes one inter-group gap while a one-group range includes none.

- [ ] **Step 2: Run the partition tests and confirm RED**

  Run: `cargo test app::tests::ordered_partition`

  Expected: compilation fails because the partition helper does not exist.

- [ ] **Step 3: Implement the minimum contiguous balanced partition**

  Preserve group order, cover every index exactly once, minimize the maximum range score using the explicit between-groups-only gap contract, and choose the lexicographically earliest complete boundary vector for equal optimum scores. The helper requires finite non-negative block heights, a finite non-negative gap, a non-empty group list, and `1..=group_count` columns; fail clearly when a precondition is broken. Return exactly the selected number of non-empty ranges.

- [ ] **Step 4: Run the partition tests and confirm GREEN**

  Run: `cargo test app::tests::ordered_partition`

  Expected: all partition tests pass.

- [ ] **Step 5: Add failing renderer-level integration tests**

  Add these tests against the production `show_shortcut_columns` entry point, using `egui::Context::run`, the installed Korean fonts, and the canonical in-memory fixture:

  - `renderer_consumes_canonical_prepared_layout`
  - `renderer_shapes_use_column_action_and_combo_clip_rects`
  - `renderer_hover_shows_exact_full_action_and_combo`
  - `renderer_maximum_style_remains_contained`

  The first test reads `ShortcutRenderReport` and requires three columns, `[0..2, 2..4, 4..6]`, one-row multi-column galleys, the measured group-height contract, and row rectangles contained by their assigned column. The shape test inspects `FullOutput.shapes`: action text shapes must carry the action rectangle clip, and every matching keycap background, border, and glyph shape must carry the combo rectangle clip. The hover test sets tooltip delay to zero, uses report rectangles to move the pointer over the canonical overflowed action and combo in separate frames, and finds the exact original strings in the tooltip text shapes. The maximum-style test uses `24px` action text, `18px` keycap text, `16px` keycap gap, `32px` action gap, `220px` configured combo width, and both `-6px` and `+6px` action offsets; it requires a non-negative action width and contained output after the column count is reduced.

- [ ] **Step 6: Run renderer integration tests and confirm RED**

  Run: `cargo test app::tests::renderer_`

  Expected: compilation or assertions fail because the production renderer does not return a report, apply row-specific clip rectangles, attach observable tooltips, or consume the prepared adaptive layout.

- [ ] **Step 7: Integrate the prepared layout into `show_shortcut_columns`**

  Remove the `980/720/480` breakpoint branch and modulo group placement. Prepare the canonical layout path once per render, calculate group blocks without trailing gaps, prepare final row galleys once, draw only non-empty columns through column-specific clips, paint action and combo shapes through their row-specific clips, attach tooltips to the allocation responses, and populate `ShortcutRenderReport` from the geometry actually consumed. Retain the existing divider and vertical scroll behavior.

- [ ] **Step 8: Run all app tests**

  Run: `cargo test app::tests::`

  Expected: all app tests pass with no warnings.

- [ ] **Step 9: Commit Task 4**

  Stage only `src/app.rs` and commit with message `feat: 그룹 순서를 보존해 오버레이 균형 배치`.

---

### Task 5: Full verification and visual acceptance

**Files:**
- Modify only if needed for test-driven corrections: `src/app.rs`
- Update tracking checkboxes: `docs/superpowers/plans/2026-07-11-adaptive-overlay-layout.md`

**Interfaces:**
- Consumes: the completed adaptive layout implementation.
- Produces: verified debug and release builds, clean formatting and clippy output, complete automated tests, and visual evidence for the long-description fixture.

- [ ] **Step 1: Run formatting verification**

  Run: `cargo fmt -- --check`

  Expected: exit code 0 with no diff.

- [ ] **Step 2: Run clippy as an error gate**

  Run: `cargo clippy --all-targets -- -D warnings`

  Expected: exit code 0 with no warnings.

- [ ] **Step 3: Run the full test suite**

  Run: `cargo test`

  Expected: every existing and new test passes with zero failures.

- [ ] **Step 4: Build the release binary**

  Run: `cargo build --release`

  Expected: exit code 0 and a worktree-local `target/release/cheatsheets.exe`.

- [ ] **Step 5: Verify the long-description layout visually**

  Use the current local `codex-acp` custom sheet as the live long-content sample; do not edit it. Before launch, stop other CheatSheets instances, record SHA-256 hashes for that sheet and `settings.json`, and save an exact temporary backup of `settings.json` because resizing updates window placement. Launch the worktree release binary, foreground Codex so this app sheet is selected, and capture four uncommitted review artifacts outside the repository: the overlay at `1755×873`, the overlay at `920×560`, one truncated action tooltip, and one clipped combo tooltip. If the expected `codex-acp` sheet is absent, stop and request a fixture instead of substituting another app silently.

  Confirm the wide view selects three columns, the narrow view reduces columns before losing readable action width, no text or keycap crosses a divider, tooltips expose the exact truncated action/combo content, one-column actions use at most two rows, configured row-height rhythm remains intact, and group order remains contiguous. Close the verification binary, restore `settings.json` byte-for-byte from its backup, and verify both final hashes match their pre-launch values. Do not add the screenshots or temporary backup to Git.

- [ ] **Step 6: Inspect the final repository scope**

  Run: `git diff --check`, `git status --short --branch -uall`, and a per-file diff review.

  Expected: only `src/app.rs` and this plan's checkbox state differ from the committed design baseline; no settings, storage schema, app-sheet, platform, or unrelated files change.

- [ ] **Step 7: Commit verification tracking if it changed**

  Stage the plan and any final test-driven correction in its own coherent commit. Use message `docs: 적응형 오버레이 검증 결과 반영` when only plan tracking changed.

## Final Review Gate

- The fixed overlay breakpoint branch is gone.
- Text and keycap measurement use active egui fonts.
- The selected column count respects group count and target action width.
- Column count is monotonic across increasing width samples and stable at exact transition boundaries.
- Production renderer output proves that columns, actions, and combo backgrounds, borders, and glyphs use their assigned clip rectangles.
- Multi-column action rows are one line; single-column action rows are at most two lines.
- Positive and negative optical offsets remain vertically contained at maximum font size.
- Hovering truncated action and combo responses renders the exact original text.
- Ordered groups are partitioned contiguously, match brute-force optima on small fixtures, and use deterministic earliest boundaries for ties.
- The canonical Korean long-content fixture produces three columns and `[0..2, 2..4, 4..6]` at `1715px` inner width.
- No persisted schema or exact key changes exist.
- Formatting, clippy, full tests, release build, and manual visual verification all pass.
