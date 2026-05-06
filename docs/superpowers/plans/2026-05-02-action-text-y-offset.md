# Action Text Y Offset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an overlay display setting that adjusts only shortcut action text vertical offset.

**Architecture:** Store `action_text_y_offset` in `OverlayStyleSettings`, normalize and serialize it with existing settings, carry it through `ResolvedOverlayStyle`, expose it in the overlay layout settings card, and use it when painting action text. Keep keycap text positioning unchanged.

**Tech Stack:** Rust, serde, eframe/egui 0.34, existing cargo tests.

---

## Files

- Modify `src/storage.rs`: add setting field/default/normalize.
- Modify `src/app.rs`: add resolved style field, UI row, action text position helper argument, tests.
- Modify `tests/app_settings.rs`: add serialization/normalization expectations.

## Task 1: Storage And Normalization

- [ ] Write failing tests in `tests/app_settings.rs` for default/serialization/normalization of `action_text_y_offset`.
- [ ] Write a failing backward-compatibility test that deserializes existing settings JSON whose `overlay_style` omits `action_text_y_offset`, then assert the loaded value defaults to `-0.75`.
- [ ] Add `pub action_text_y_offset: f32` to `OverlayStyleSettings`.
- [ ] Set default to `-0.75`.
- [ ] Normalize with `normalize_f32(value, -6.0, 6.0, defaults.action_text_y_offset)`.
- [ ] Run focused tests.

## Task 2: Render Pipeline And UI

- [ ] Write failing tests in `src/app.rs` for resolved style carrying `action_text_y_offset` and action text position using the configured offset.
- [ ] Add/update a test proving changing action text offset does not change `keycap_text_position`.
- [ ] Add a reset/default verification proving `OverlayStyleSettings::default().action_text_y_offset == -0.75`, which is the value restored by the existing overlay style reset path.
- [ ] Add `action_text_y_offset` to `ResolvedOverlayStyle` and `resolved_overlay_style`.
- [ ] Change `shortcut_action_text_position(rect)` to `shortcut_action_text_position(rect, offset)`.
- [ ] Keep `keycap_text_position` unchanged.
- [ ] Use `style.action_text_y_offset` in `show_shortcut_action_text`.
- [ ] Add `동작 세로 보정` row under overlay `레이아웃` using `overlay_style_number_row(..., -6.0..=6.0, "px")`.
- [ ] Run focused tests.

## Task 3: Full Verification

- [ ] Run `cargo fmt -- --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test`.
- [ ] Run `cargo check`.
- [ ] If release exe is locked, stop only the worktree release process, then run `cargo build --release`.
