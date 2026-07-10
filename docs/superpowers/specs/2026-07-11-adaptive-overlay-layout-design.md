# Adaptive Overlay Layout Design

**Status:** The design direction was approved in conversation on 2026-07-11. This document is the written review artifact.

## Purpose

Make the shortcut overlay remain readable and visually composed when shortcut descriptions, key combinations, group counts, font sizes, and window sizes vary.

The primary user task is to open the overlay and scan the available shortcuts quickly without text crossing column boundaries or the user manually tuning the column count.

This specification supersedes the fixed overlay breakpoint and fixed-width text assumptions in [2026-05-01-overlay-layout-design.md](2026-05-01-overlay-layout-design.md) and [2026-05-01-overlay-style-settings-design.md](2026-05-01-overlay-style-settings-design.md) when they conflict with this design.

## Problem

The current overlay selects one to four equal-width columns from fixed available-width breakpoints. The calculation does not consider the number of groups, measured action text, configured key area width, font sizes, or group height.

Each action row reserves a rectangle but paints the description as an unwrapped line without truncation or a rectangle-specific clip. Long text can therefore paint over the divider and the next column. The current minimum window width also makes the old one- and two-column breakpoints effectively unreachable.

The existing keycap width formula is based on character count rather than the configured font and rendered glyph width. Large keycap text and long labels can overflow their badges or consume the action area unpredictably.

## Goals

- Select the highest useful column count from the actual content and resolved overlay style.
- Keep every painted action and keycap inside its assigned column.
- Preserve a stable single-line scanning rhythm in multi-column layouts.
- Preserve full shortcut information through tooltips and a limited two-line single-column presentation.
- Preserve explicit app-sheet group order while balancing column heights.
- Keep existing card colors, typography roles, keycap treatment, settings schema, file formats, import behavior, and hotkey behavior.
- Make layout behavior deterministic and testable without pixel snapshots.

## Non-Goals

- Redesigning the settings popup, tray menu, shortcut editor, import workflow, or status messages.
- Adding a manual column-count setting.
- Changing app-sheet JSON or CSV contracts.
- Introducing per-app visual settings.
- Replacing the existing vertical scroll behavior.
- Adding persistent layout caches before profiling demonstrates a need.

## Design Principles

### System-managed complexity

The system chooses columns and containment behavior. Users may continue to choose density-related style values, but they do not need to understand breakpoints or calculate a safe column count.

### Stable scan rhythm

Multi-column rows remain one line high. Rare outliers use truncation and a tooltip instead of making neighboring rows jump vertically.

### Hard containment

Layout measurement improves the common case, while explicit clipping protects every edge case. A long Korean string, English sentence, path, command, or unbroken token must never paint into another column.

### Ordered balance

Group order remains meaningful. Balancing must not reorder groups merely to fill empty space.

## Layout Measurement Contracts

All measurements use the resolved egui fonts and the current `ResolvedOverlayStyle`. Character-count heuristics are not used for visible text or keycap badge width.

### Action width target

1. Measure every visible action as a single line at the configured action font size.
2. Sort the measured widths and select the nearest-rank 90th percentile.
3. Clamp that value to `180..=320` logical pixels.

The lower bound preserves a readable phrase. The upper bound prevents one content-heavy sheet from collapsing unnecessarily to a single column. Text beyond the target remains available through the overflow policy.

### Combo rail target

1. Split each combo by the existing `+` contract.
2. Measure each keycap label with the configured keycap font, horizontal badge padding, and configured inter-keycap gap.
3. Measure the full keycap run for every visible shortcut.
4. Select the nearest-rank 90th percentile of those run widths.
5. Use the larger of that value and the configured `combo_width`, capped at `220` logical pixels.

The configured `combo_width` remains the user's preferred minimum alignment rail. The rail may grow to fit common content but never exceeds the existing safe maximum. An outlier wider than the rail follows the combo overflow policy.

### Minimum useful column width

The minimum useful column width consists of:

- target combo rail width;
- current horizontal widget spacing;
- configured action gap;
- target action width.

The column gap used by the column container is included when calculating how many columns fit.

### Column count

The selected column count is the greatest count that satisfies all of these constraints:

- at least one column;
- no more than four columns;
- no more columns than visible groups;
- every selected column is at least the minimum useful column width.

If even one minimum useful column does not fit, the layout still uses one column and applies the overflow policies. Widening the window may add a column only when the new columns retain the target action width; it must not produce an unreadably narrow action area.

## Text and Keycap Overflow Policies

### Action descriptions

- Two or more columns: render one line, truncate with an ellipsis, and expose the complete description in a hover tooltip only when truncation occurs.
- One column: allow up to two lines. Content beyond the second line is truncated, and the complete description is exposed in a hover tooltip.
- Apply an explicit clip rectangle equal to the action area in every mode.
- Use the actual laid-out text height to vertically align one- and two-line content with its keycap rail.
- Treat whitespace-free strings exactly like normal text; they do not receive an overflow exception.

### Keycap runs

- Derive every badge width from measured glyph width plus horizontal padding.
- Keep the keycap run right-aligned inside the combo rail.
- Clip a run that exceeds the combo rail instead of shrinking the configured font.
- Preserve the final key and clip the left side first when an oversized run is right-aligned.
- Expose the complete combo in a hover tooltip when clipping occurs.
- Preserve the current keycap colors, radius, height, and inter-keycap gap.

## Group Distribution

Groups remain atomic blocks, including when a single group is taller than the available scroll viewport. The layout partitions the ordered group list into contiguous column ranges and relies on the existing vertical scroll area for a tall group.

The partition minimizes the tallest estimated column while preserving the source group order. Estimated height includes the group heading, heading gap, all measured row heights after applying the text policy, and the inter-group gap. Equal-score partitions use the earliest valid boundary so the result remains deterministic.

This replaces modulo-based round-robin assignment. Reading top-to-bottom within a column and then moving right preserves the app sheet's explicit `group_order`.

Only non-empty columns receive dividers. Divider placement uses the final selected column geometry.

## Rendering Flow

1. Resolve the current overlay style.
2. Group shortcuts in the existing app-sheet order.
3. Measure action text and keycap runs with the active egui fonts.
4. Calculate target action width, combo rail width, and column count.
5. Estimate group heights and partition groups into ordered balanced columns.
6. Render headings and rows using the selected text policy.
7. Apply column, action, and combo clip rectangles as final containment.
8. Keep the existing vertical scroll area for content taller than the viewport.

No storage, import, catalog, or platform data flow changes are required.

## Settings Compatibility

- Existing `settings.json` files load without migration.
- `combo_width` remains the preferred minimum combo rail width.
- `action_text_size`, `keycap_text_size`, `row_height`, `keycap_height`, `keycap_gap`, `action_gap`, and card padding participate in measurement.
- Existing normalization ranges remain unchanged.
- No new fallback keys, legacy aliases, or alternative setting names are introduced.

## Visual Direction

The current warm off-white card, subtle dividers, compact typography, and keycap styling remain the visual foundation.

The improvement comes from wider readable action areas, predictable alignment, balanced group blocks, and clean truncation. The overlay should feel calmer and more deliberate without adding decoration or changing its visual identity.

## Edge Cases

- Empty sheet: retain the current empty state behavior.
- One group: use one column regardless of window width.
- Fewer than four groups: never create empty layout columns.
- One exceptionally long action: percentile clamping prevents it from controlling all columns; truncation and tooltip preserve access.
- One exceptionally long combo or command-like value: combo rail cap, clipping, and tooltip preserve the grid.
- Maximum configured font and gap values: the layout reduces column count before allowing a negative or unusable action width.
- Very tall group: keep it intact and rely on vertical scrolling rather than splitting it unpredictably.
- Non-finite layout input: continue relying on existing style normalization before measurement.

## Testing Strategy

Automated tests must cover pure layout decisions and egui-backed text containment without depending on the user's external app sheets.

### Column selection

- Column count is always within `1..=min(4, group_count)`.
- A one-group sheet always uses one column.
- The current long-description scenario at `1755px` selects three columns rather than four.
- A wide short-description sheet may use four columns.
- Narrow widths reduce the count before the computed action area drops below its target.
- Values immediately around a column transition retain a usable action area.

### Measurement

- Keycap width increases with actual font size and glyph width.
- Measured combo runs include configured keycap gaps.
- Action target uses the deterministic nearest-rank 90th percentile and clamps to `180..=320`.
- Combo target honors configured `combo_width` and caps at `220`.

### Containment

- Long Korean, English, mixed-language, path-like, and whitespace-free actions remain inside the action clip rectangle.
- Multi-column text is one line and reports truncation when needed.
- Single-column text uses no more than two lines and reports truncation when needed.
- Long combo runs remain inside the combo rail and report clipping.
- Full action and combo text remains available for the tooltip response.

### Group distribution

- Every group appears exactly once.
- Group order is preserved across contiguous column ranges.
- The partition is deterministic for equal-height groups.
- The maximum column height is no worse than a simple contiguous equal-count split for representative uneven groups.

### Regression

- Existing overlay style, storage, import, hotkey, tray, and window-placement tests remain green.
- Manual verification uses representative short sheets and a fixture matching the current long `codex-acp` descriptions at wide and minimum window sizes.

## Acceptance Criteria

- No action text, keycap glyph, or keycap background paints across its assigned rail or column boundary.
- The current long-description fixture renders without overlapping another column or divider.
- Column count responds to content, style, window width, and group count rather than fixed width breakpoints alone.
- Adding a column never leaves less than the calculated target action width.
- Multi-column rows remain visually aligned and one line high.
- Full truncated content is available through a tooltip.
- Ordered groups remain in source order and column heights are materially more balanced than modulo assignment.
- Existing settings and app-sheet files require no migration.
- Automated tests and manual visual verification pass before integration.
