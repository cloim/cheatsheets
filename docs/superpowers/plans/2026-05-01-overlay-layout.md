# Overlay Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Pixelmator-style light shortcut overlay with keycap badge combos and vertically aligned descriptions.

**Architecture:** Keep all UI changes in `src/app.rs`, where the existing overlay and shortcut column rendering already live. Add small helper functions for keycap measurement/rendering and a shortcut-card palette, without changing shortcut storage, import, or platform code.

**Tech Stack:** Rust 2024, `eframe`/`egui`, existing `cargo test` and `cargo fmt` workflow.

---

## File Structure

- Modify `src/app.rs`: shortcut view layout, card styling, keycap badge rendering helpers, and focused tests for combo splitting/width constants where practical.
- No new runtime files or image assets.
- No storage, import, platform, or settings data model changes.

### Task 1: Keycap Combo Helpers

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add tests for combo splitting**

Add focused unit tests near the existing `app.rs` tests:

```rust
#[test]
fn combo_parts_trim_empty_segments() {
    assert_eq!(combo_keycap_parts(" Ctrl + Shift + P "), vec!["Ctrl", "Shift", "P"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test combo_parts_trim_empty_segments`
Expected: FAIL because `combo_keycap_parts` does not exist.

- [ ] **Step 3: Implement minimal helper**

Add a helper in `src/app.rs` near shortcut rendering helpers:

```rust
fn combo_keycap_parts(combo: &str) -> Vec<&str> {
    combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test combo_parts_trim_empty_segments`
Expected: PASS.

### Task 2: Reference-Style Overlay Card

**Files:**
- Modify: `src/app.rs:556-593`
- Modify: `src/app.rs:1072-1107`

- [ ] **Step 1: Add card constants/helpers**

Add constants for card margin, padding, fill, border, text, muted text, keycap fill, keycap border, and divider colors near UI helper functions. Keep them local to `app.rs`; shortcut card rendering should not reuse dark-mode `UiPalette` text colors.

- [ ] **Step 2: Account for existing global inset**

The root `ui()` already shrinks all content by `28px`. To reach the spec's roughly `42px` outer margin, use an additional shortcut-card inset of about `14px` from the already-shrunk rect, not another full `42px` inset.

- [ ] **Step 3: Replace shortcut-view background with card treatment**

For `AppView::Shortcuts`, draw a rounded off-white translucent card inset from `ui.max_rect()` and place shortcut contents inside the card. Leave `AppView::Settings` on the existing theme/palette path.

- [ ] **Step 4: Apply inner content padding**

After drawing the card, create the shortcut content UI from `card_rect.shrink(24.0)` so headings and columns align to the spec's inner padding.

- [ ] **Step 5: Preserve empty-state behavior**

Keep the existing empty shortcut message, but render it inside the card with muted light-card text.

- [ ] **Step 6: Compile check**

Run: `cargo check`
Expected: PASS.

### Task 3: Compact Columns With Dividers

**Files:**
- Modify: `src/app.rs:788-835`

- [ ] **Step 1: Update column thresholds and spacing**

Use dense thresholds for 4/3/2/1 columns and reduce group/row spacing to match the reference. Set group headings around `13px`, shortcut action text around `12px`, and shortcut row height around `18px`.

- [ ] **Step 1a: Update heading font test**

Update `group_heading_uses_bold_korean_font_family` to assert the new group heading size around `13.0` instead of the current `16.0`.

- [ ] **Step 2: Draw vertical dividers**

Inside the columns callback, draw a faint vertical line at the right edge of each column except the last visible column.

- [ ] **Step 3: Keep grouped order unchanged**

Do not change `grouped_shortcuts`; grouping and order must stay data-driven.

- [ ] **Step 4: Compile check**

Run: `cargo check`
Expected: PASS.

### Task 4: Keycap Badge Rows

**Files:**
- Modify: `src/app.rs:815-831`

- [ ] **Step 1: Add `show_keycap_combo` helper**

Render combo parts as small rounded rectangles with subtle border/fill and centered text using the shortcut-card palette, not the theme-dependent `UiPalette`.

- [ ] **Step 2: Right-align combo badges**

Allocate a fixed combo area around `112px`, calculate total badge width, and start the badges so their right edge is stable.

- [ ] **Step 3: Align action labels**

Render the action label after a fixed gap so descriptions start at the same x-position in every row.

- [ ] **Step 4: Run focused and full tests**

Run: `cargo test combo_parts_trim_empty_segments`
Expected: PASS.

Run: `cargo test`
Expected: PASS.

### Task 5: Formatting And Final Verification

**Files:**
- Verify: all modified files

- [ ] **Step 1: Format code**

Run: `cargo fmt`
Expected: completes without errors.

- [ ] **Step 2: Check formatting**

Run: `cargo fmt -- --check`
Expected: PASS.

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 4: Inspect working tree**

Run: `git status --short`
Expected: only intended source and docs changes are present.

### Task 6: Frameless Overlay Window

**Files:**
- Modify: `src/main.rs:17-24`
- Modify: `src/app.rs:556-612`
- Modify: `src/app.rs:930-963`

- [ ] **Step 1: Remove native window chrome**

Change the viewport builder to use `.with_decorations(false)` and `.with_resizable(false)` while keeping transparency and always-on-top behavior.

- [ ] **Step 2: Add internal drag movement**

In shortcut overlay rendering, make the light card respond to drag and send `egui::ViewportCommand::StartDrag` so the overlay can move without a native title bar.

- [ ] **Step 3: Left-align action descriptions**

Set the action label layout to left alignment in the fixed action area so descriptions do not appear centered.

- [ ] **Step 4: Verify**

Run the release app and visually confirm the overlay has no native title bar, no resize border, can be moved by dragging the internal card, and action descriptions are left-aligned.

If automated screenshot capture is unavailable in the current session, state that limitation and report the executable/process state instead of claiming visual verification.

- [ ] **Step 5: Run automated checks**

Run: `cargo fmt -- --check`
Expected: PASS.

Run: `cargo test`
Expected: PASS.

Run: `cargo build --release`
Expected: PASS after the running release executable is stopped if needed.

### Task 7: Remove Shortcut Overlay Black Border

**Files:**
- Modify: `src/app.rs:556-612`
- Modify: `src/app.rs:1230-1265`

- [ ] **Step 1: Add regression test**

Add a test proving `AppView::Shortcuts` uses `0px` content inset and `0px` card outer inset so no transparent margin appears around the card.

- [ ] **Step 2: Remove shortcut-only outer margins**

Use `0px` root content inset and `0px` card outer inset for `AppView::Shortcuts`, while keeping existing margins for settings.

- [ ] **Step 3: Verify**

Run: `cargo fmt -- --check`
Expected: PASS.

Run: `cargo test`
Expected: PASS.

Run: `cargo build --release`
Expected: PASS after the running release executable is stopped if needed.

### Task 8: Add Overlay Resize Grip

**Files:**
- Modify: `src/app.rs:574-638`
- Modify: `src/app.rs` shortcut UI helper section

- [ ] **Step 1: Add resize helper tests**

Add tests proving the resize grip is anchored at the bottom-right corner and resize deltas clamp to `WindowPlacement::MIN_WIDTH`/`MIN_HEIGHT`.

- [ ] **Step 2: Draw bottom-right grip**

Render a subtle diagonal-line grip inside the shortcut card's bottom-right corner.

- [ ] **Step 3: Resize viewport on grip drag**

When the grip is dragged, send `egui::ViewportCommand::InnerSize` with the current viewport size plus the drag delta, clamped to the existing minimum size.

- [ ] **Step 4: Prevent drag conflicts**

Ensure card move-drag does not start from the resize grip area.

- [ ] **Step 5: Verify**

Run: `cargo fmt -- --check`
Expected: PASS.

Run: `cargo test`
Expected: PASS.

Run: `cargo build --release`
Expected: PASS after the running release executable is stopped if needed.
