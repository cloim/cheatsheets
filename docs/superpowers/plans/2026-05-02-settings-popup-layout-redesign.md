# Settings Popup Layout Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the settings popup layout so it uses the window width effectively, keeps key actions visible, and presents overlay display settings as compact card-based groups instead of one long form.

**Architecture:** Keep the existing settings popup viewport and settings state unchanged. Add small, testable layout helper functions, then refactor the settings rendering helpers in `src/app.rs` to use a sidebar/header/content structure and compact card controls. Do not change storage schema, tray behavior, popup lifecycle, hotkey capture semantics, or overlay rendering.

**Tech Stack:** Rust, eframe/egui 0.34, existing `UiPalette`, existing `OverlayStyleSettings`, cargo fmt/clippy/test/check.

---

## File Structure

- Modify `src/app.rs`: settings popup layout helpers, sidebar styling, section header, settings cards, compact numeric/color/toggle rows, tests for non-visual layout decisions.
- No changes expected in `src/storage.rs`: existing settings data model is sufficient.
- No changes expected in `tests/app_settings.rs`: existing serialization/normalization tests should continue to pass.
- Spec reference: `docs/superpowers/specs/2026-05-02-settings-popup-layout-redesign.md`.

Do not commit unless the user explicitly requests it. At checkpoints, inspect status/diff only.

## Task 1: Add Testable Layout Constants And Helpers

**Files:**
- Modify: `src/app.rs:99-110`
- Modify: `src/app.rs:1374-1424`
- Test: `src/app.rs:1792-2167`

- [ ] **Step 1: Write failing tests for layout decisions**

Add tests to the existing `#[cfg(test)] mod tests` in `src/app.rs`:

```rust
#[test]
fn settings_popup_default_size_supports_two_column_card_layout() {
    let viewport = settings_popup_viewport();

    assert_eq!(viewport.inner_size, Some(egui::vec2(1080.0, 680.0)));
    assert_eq!(settings_card_column_count(settings_content_width(1080.0)), 2);
}

#[test]
fn settings_popup_minimum_size_stacks_cards() {
    let viewport = settings_popup_viewport();

    assert_eq!(viewport.min_inner_size, Some(egui::vec2(760.0, 560.0)));
    assert_eq!(settings_card_column_count(settings_content_width(760.0)), 1);
}

#[test]
fn settings_cards_use_two_columns_when_space_allows() {
    assert_eq!(settings_card_column_count(760.0), 2);
}

#[test]
fn settings_cards_stack_when_narrow() {
    assert_eq!(settings_card_column_count(620.0), 1);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test settings_popup_default_size_supports_two_column_card_layout
cargo test settings_popup_minimum_size_stacks_cards
cargo test settings_cards_use_two_columns_when_space_allows
cargo test settings_cards_stack_when_narrow
```

Expected: fail because `settings_content_width` and `settings_card_column_count` do not exist and viewport sizes still use the old dimensions.

- [ ] **Step 3: Add constants and helper**

Near the settings viewport helpers, add constants and helper logic:

```rust
const SETTINGS_SIDEBAR_WIDTH: f32 = 208.0;
const SETTINGS_CARD_GAP: f32 = 8.0;
const SETTINGS_TWO_COLUMN_MIN_WIDTH: f32 = 720.0;

#[cfg(test)]
fn settings_content_width(inner_width: f32) -> f32 {
    (inner_width - SETTINGS_SIDEBAR_WIDTH - 32.0).max(0.0)
}

fn settings_card_column_count(content_width: f32) -> usize {
    if content_width >= SETTINGS_TWO_COLUMN_MIN_WIDTH {
        2
    } else {
        1
    }
}
```

Update `settings_popup_viewport()` to use default inner size `[1080.0, 680.0]` and minimum inner size `[760.0, 560.0]`. The default width must leave enough right-panel content width for two overlay cards after subtracting the fixed sidebar and separators. Keep the window decorated, resizable, opaque, and always-on-top.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test settings_popup_default_size_supports_two_column_card_layout
cargo test settings_popup_minimum_size_stacks_cards
cargo test settings_cards_use_two_columns_when_space_allows
cargo test settings_cards_stack_when_narrow
```

Expected: pass.

- [ ] **Step 5: Checkpoint**

Run:

```bash
git status --short
git diff -- src/app.rs
```

Expected: only intentional helper/test changes in `src/app.rs`.

## Task 2: Restructure Popup Into Sidebar, Header, And Content Panel

**Files:**
- Modify: `src/app.rs:481-509`
- Modify: `src/app.rs:1410-1424`
- Test: existing tests in `src/app.rs`

- [ ] **Step 1: Add a test for section descriptions**

Add a test:

```rust
#[test]
fn settings_section_descriptions_are_not_empty() {
    for section in settings_sections() {
        assert!(!section.description().is_empty());
    }
}
```

Expected: fail until `SettingsSection::description()` exists.

- [ ] **Step 2: Add section description helper**

Implement:

```rust
impl SettingsSection {
    fn description(self) -> &'static str {
        match self {
            Self::General => "앱 화면과 전체 표시 방식을 설정합니다.",
            Self::OverlayDisplay => "오버레이 카드의 표시 스타일과 레이아웃을 설정합니다.",
            Self::Hotkeys => "오버레이를 여닫는 전역 단축키를 설정합니다.",
            Self::ShortcutEditor => "현재 앱의 사용자 단축키를 추가하거나 수정합니다.",
        }
    }
}
```

- [ ] **Step 3: Replace popup root layout**

Refactor `show_settings_popup_contents` from the current simple horizontal split into:

```rust
fn show_settings_popup_contents(
    &mut self,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    palette: UiPalette,
) {
    let available = ui.available_size();
    ui.set_min_size(available);
    ui.horizontal(|ui| {
        ui.set_min_height(available.y);
        self.show_settings_sidebar(ui, palette);
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            let mut reset_overlay = false;
            self.show_settings_header(ui, palette, &mut reset_overlay);
            ui.add_space(12.0);
            if reset_overlay {
                self.settings.overlay_style = OverlayStyleSettings::default();
                self.settings.overlay_style.normalize();
                self.persist_settings();
                ctx.request_repaint();
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.settings_section {
                    SettingsSection::General => self.show_settings_general(ui, ctx, palette),
                    SettingsSection::OverlayDisplay => self.show_settings_overlay_display(ui, ctx),
                    SettingsSection::Hotkeys => self.show_settings_hotkeys(ui, palette),
                    SettingsSection::ShortcutEditor => self.show_settings_shortcut_editor(ui, palette),
                });
        });
    });
}
```

Adjust as needed to satisfy borrow checking. The reset action can be handled by returning `bool` from `show_settings_header` instead of mutating through an argument.

- [ ] **Step 4: Implement header helper**

Add `show_settings_header` as an `impl CheatSheetsApp` helper. It should render:

- current section label as a large heading
- current section description in weak text
- `기본값으로 되돌리기` button only when `settings_section == SettingsSection::OverlayDisplay`

Do not add previous/next navigation.

- [ ] **Step 5: Restyle sidebar without changing behavior**

Change `show_settings_sidebar` to accept `palette: UiPalette`, use `SETTINGS_SIDEBAR_WIDTH`, and render each section as a fixed-height rounded selectable row. Use existing `settings_sections()` order and update `self.settings_section` on click.

Keep icons optional. Do not add assets.

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test settings_section_labels_are_stable
cargo test settings_section_defaults_to_general
cargo test settings_section_descriptions_are_not_empty
```

Expected: pass.

## Task 3: Add Reusable Card And Compact Row Helpers

**Files:**
- Modify: `src/app.rs:1374-1424`
- Test: existing tests in `src/app.rs`

- [ ] **Step 1: Add card helper**

Replace or supplement `settings_section` with a card helper. Keep the old helper if it is still useful for smaller sections.

```rust
fn settings_card(
    ui: &mut egui::Ui,
    title: &str,
    palette: UiPalette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::NONE
        .fill(palette.extreme_bg)
        .stroke(egui::Stroke::new(1.0, palette.button_bg))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(20, 18))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(15.0)
                    .strong()
                    .color(palette.heading),
            );
            ui.add_space(10.0);
            add_contents(ui);
        });
}
```

Use existing `UiPalette` fields. Do not reference nonexistent `card` or `divider` fields unless the implementation intentionally adds those fields and updates both dark and light palette constructors.

- [ ] **Step 2: Add compact numeric row helper**

Replace `overlay_style_slider` usage with a compact numeric editor. A minimal helper can be:

```rust
fn overlay_style_number_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(suffix);
            changed |= ui
                .add(
                    egui::DragValue::new(value)
                        .range(range)
                        .speed(0.5)
                        .max_decimals(1),
                )
                .changed();
        });
    });
    changed
}
```

Keep ranges identical to the existing sliders.

- [ ] **Step 3: Update color row helper**

Refactor `rgba_color_edit` to keep the same signature but use a right-aligned compact color button:

```rust
fn rgba_color_edit(ui: &mut egui::Ui, label: &str, color: &mut RgbaColor) -> bool {
    let mut egui_color = rgba_to_color(*color);
    let changed = ui
        .horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.color_edit_button_srgba(&mut egui_color).changed()
            })
            .inner
        })
        .inner;
    if changed {
        *color = RgbaColor::rgba(
            egui_color.r(),
            egui_color.g(),
            egui_color.b(),
            egui_color.a(),
        );
    }
    changed
}
```

- [ ] **Step 4: Add toggle row helper**

Add:

```rust
fn settings_toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(34.0);
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed |= ui.checkbox(value, "").changed();
        });
    });
    changed
}
```

- [ ] **Step 5: Run compile check**

Run:

```bash
cargo check
```

Expected: pass or show only API naming issues to fix in the helper code.

## Task 4: Convert Overlay Display To Responsive Card Grid

**Files:**
- Modify: `src/app.rs:551-706`
- Modify: `src/app.rs:1374-1424`
- Test: existing tests in `src/app.rs`

- [ ] **Step 1: Split overlay display into card helpers**

Inside `impl CheatSheetsApp`, add private helpers if they reduce borrow complexity:

```rust
fn show_overlay_text_and_layout_card(&mut self, ui: &mut egui::Ui, palette: UiPalette) -> bool
fn show_overlay_color_and_options_card(&mut self, ui: &mut egui::Ui, palette: UiPalette) -> bool
```

Each helper returns whether any setting changed.

- [ ] **Step 2: Implement text and layout card**

Use one `settings_card(ui, "텍스트 크기", ...)` section for font sizes and one internal heading/divider for `레이아웃`, or one card titled `텍스트 크기 / 레이아웃` if the layout is cleaner.

Replace each slider call with `overlay_style_number_row` using existing ranges:

```rust
changed |= overlay_style_number_row(ui, "제목 크기", &mut self.settings.overlay_style.title_size, 10.0..=36.0, "px");
changed |= overlay_style_number_row(ui, "설명 크기", &mut self.settings.overlay_style.subtitle_size, 8.0..=24.0, "px");
changed |= overlay_style_number_row(ui, "그룹 크기", &mut self.settings.overlay_style.group_heading_size, 8.0..=24.0, "px");
changed |= overlay_style_number_row(ui, "동작 크기", &mut self.settings.overlay_style.action_text_size, 8.0..=24.0, "px");
changed |= overlay_style_number_row(ui, "키캡 글자", &mut self.settings.overlay_style.keycap_text_size, 7.0..=18.0, "px");
```

Then layout rows:

```rust
changed |= overlay_style_number_row(ui, "안쪽 여백", &mut self.settings.overlay_style.card_padding, 0.0..=64.0, "px");
changed |= overlay_style_number_row(ui, "행 높이", &mut self.settings.overlay_style.row_height, 12.0..=40.0, "px");
changed |= overlay_style_number_row(ui, "키 영역 너비", &mut self.settings.overlay_style.combo_width, 64.0..=220.0, "px");
changed |= overlay_style_number_row(ui, "동작 간격", &mut self.settings.overlay_style.action_gap, 0.0..=32.0, "px");
changed |= overlay_style_number_row(ui, "키캡 높이", &mut self.settings.overlay_style.keycap_height, 10.0..=28.0, "px");
changed |= overlay_style_number_row(ui, "키캡 간격", &mut self.settings.overlay_style.keycap_gap, 0.0..=16.0, "px");
changed |= overlay_style_number_row(ui, "모서리", &mut self.settings.overlay_style.card_radius, 0.0..=24.0, "px");
```

- [ ] **Step 3: Implement color and options card**

Use `settings_card(ui, "색상", ...)` with compact `rgba_color_edit` rows for all existing colors. Add an internal `표시 옵션` heading/divider and use `settings_toggle_row` for the three booleans.

- [ ] **Step 4: Implement responsive card layout**

Refactor `show_settings_overlay_display` to compute content width and choose columns:

```rust
fn show_settings_overlay_display(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
    let palette = palette_for(resolved_theme(ctx, self.settings.theme));
    let mut changed = false;
    let columns = settings_card_column_count(ui.available_width());

    if columns == 2 {
        ui.columns(2, |columns| {
            changed |= self.show_overlay_text_and_layout_card(&mut columns[0], palette);
            changed |= self.show_overlay_color_and_options_card(&mut columns[1], palette);
        });
    } else {
        changed |= self.show_overlay_text_and_layout_card(ui, palette);
        ui.add_space(SETTINGS_CARD_GAP);
        changed |= self.show_overlay_color_and_options_card(ui, palette);
    }

    if changed {
        self.settings.overlay_style.normalize();
        self.persist_settings();
        ctx.request_repaint();
    }
}
```

If borrow checking rejects calling two `&mut self` helpers inside `ui.columns`, inline the rows inside the closure or split changed values into local borrows of `self.settings.overlay_style` before columns.

- [ ] **Step 5: Remove reset button from overlay body**

Ensure `기본값으로 되돌리기` exists only in the header action area and is no longer at the bottom of `show_settings_overlay_display`.

- [ ] **Step 6: Run focused checks**

Run:

```bash
cargo check
cargo test default_overlay_style_matches_current_visual_constants
cargo test resize_grip_toggle_controls_resize_interaction
cargo test divider_toggle_controls_column_dividers
cargo test empty_message_toggle_controls_empty_state
```

Expected: pass. Existing overlay defaults and behavior must be unchanged.

## Task 5: Convert Other Settings Sections To Cards

**Files:**
- Modify: `src/app.rs:511-549`
- Modify: `src/app.rs:708-765`
- Test: existing tests in `src/app.rs`

- [ ] **Step 1: Convert General section**

Replace `settings_section(ui, "화면", ...)` with a `settings_card(ui, "화면", ...)`. Keep the same theme choices and opacity setting behavior. Preserve calls to `self.settings.normalize()`, `configure_style(ctx, self.settings.theme)`, `persist_settings()`, and `ctx.request_repaint()` when changed.

- [ ] **Step 2: Convert Hotkeys section**

Replace `settings_section(ui, "오버레이 단축키", ...)` with a card. Keep `shortcut_capture_button` behavior and `self.capture_target = Some(CaptureTarget::ToggleHotkey)` on click.

Optionally add weak helper text inside the card:

```rust
if self.capture_target == Some(CaptureTarget::ToggleHotkey) {
    ui.label(egui::RichText::new("새 단축키를 누르세요. Esc는 캡처 취소/닫기 흐름을 따릅니다.").color(palette.weak_text));
}
```

- [ ] **Step 3: Convert Shortcut Editor section**

Wrap the existing editor grid in a card titled `단축키 추가 / 수정`. Keep all current controls, labels, save behavior, import behavior, and status messages. Do not refactor shortcut data flow.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test captured_key_combo_uses_display_format
cargo test parses_display_hotkey_for_registration
cargo test settings_popup_escape_does_not_close_while_capturing_shortcut
cargo test settings_popup_escape_closes_when_not_capturing_shortcut
```

Expected: pass.

## Task 6: Theme Polish And Manual Verification

**Files:**
- Modify: `src/app.rs` only if visual polish requires small helper tweaks.
- No tests unless helper behavior changes.

- [ ] **Step 1: Inspect `UiPalette` fields**

Use the existing palette fields to pick card fill, sidebar selected fill, weak text, divider, and accent colors. Do not introduce a separate full dark-only palette.

- [ ] **Step 2: Tune spacing and sizes**

Adjust constants only if needed:

```rust
const SETTINGS_SIDEBAR_WIDTH: f32 = 208.0;
const SETTINGS_CARD_GAP: f32 = 8.0;
```

Keep the layout compact enough that `오버레이 표시` uses two columns at the default popup size.

- [ ] **Step 3: Verify no footer navigation exists**

Search for added labels/buttons named `이전` or `다음` in the settings popup work:

```bash
rg '이전|다음' src/app.rs
```

Expected: no newly added settings navigation buttons. Existing unrelated occurrences, if any, should be inspected and left unchanged.

- [ ] **Step 4: Run full verification**

Run:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo check
```

Expected: all pass.

- [ ] **Step 5: Build release for manual inspection**

If a running release binary locks the executable, terminate only the worktree binary process after confirming its path, then run:

```bash
cargo build --release
```

Expected: release build succeeds.

- [ ] **Step 6: Manual layout verification**

Launch the worktree release binary and open settings. Verify:

- `오버레이 표시` uses two cards at normal popup width.
- Header shows section title/description and reset action for overlay display.
- Reset is visible without scrolling.
- There is no bottom previous/next navigation.
- Narrow resizing keeps controls usable through stacking or localized scrolling.
- Numeric edits, color edits, toggles, hotkey capture, shortcut save/import, and theme/opacity changes still persist immediately.

- [ ] **Step 7: Final status review**

Run:

```bash
git status --short
git diff --stat
```

Expected: intentional changes in `src/app.rs` and the new spec/plan docs only, unless earlier feature-branch files already had unrelated pending changes.
