# Overlay Style Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add global settings-menu controls for shortcut overlay typography, colors, layout, and useful visibility toggles.

**Architecture:** Store a nested `OverlayStyleSettings` value in `AppSettings`, normalize it on load/save, and resolve it into egui-ready overlay rendering values in `src/app.rs`. Keep shortcut data, app sheet metadata, platform code, tray behavior, and hotkey behavior unchanged.

**Tech Stack:** Rust, eframe/egui, serde/serde_json, cargo test.

---

## File Structure

- Modify `src/storage.rs`: add serializable `RgbaColor` and `OverlayStyleSettings`, include `overlay_style` in `AppSettings`, implement defaults and normalization.
- Modify `tests/app_settings.rs`: cover old JSON compatibility, serialization, and normalization for overlay style settings.
- Modify `src/app.rs`: import the new setting types, resolve overlay settings into `ShortcutCardPalette` and layout values, use settings in shortcut rendering, and add settings-menu controls.
- Keep tests inside `src/app.rs` for private rendering helpers such as palette resolution, style resolution, and toggle behavior.

## Task 1: Storage Model

**Files:**
- Modify: `src/storage.rs:43-81`
- Modify: `tests/app_settings.rs`

- [ ] **Step 1: Write old-JSON compatibility test**

Add this test to `tests/app_settings.rs`:

```rust
#[test]
fn app_settings_loads_default_overlay_style_when_missing_from_json() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{
  "theme": "Default",
  "opacity": 0.96,
  "toggle_hotkey": "Ctrl+Shift+Space",
  "window": null
}"#,
    )
    .unwrap();

    settings.normalize();

    assert_eq!(settings.overlay_style, Default::default());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test app_settings app_settings_loads_default_overlay_style_when_missing_from_json`

Expected: FAIL because `AppSettings` has no `overlay_style` field.

- [ ] **Step 3: Add storage types and defaults**

In `src/storage.rs`, add public serializable structs near `AppSettings`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayStyleSettings {
    pub title_size: f32,
    pub subtitle_size: f32,
    pub group_heading_size: f32,
    pub action_text_size: f32,
    pub keycap_text_size: f32,
    pub card_background: RgbaColor,
    pub card_border: RgbaColor,
    pub title_color: RgbaColor,
    pub group_heading_color: RgbaColor,
    pub action_text_color: RgbaColor,
    pub weak_text_color: RgbaColor,
    pub divider_color: RgbaColor,
    pub keycap_background: RgbaColor,
    pub keycap_border: RgbaColor,
    pub keycap_text_color: RgbaColor,
    pub card_padding: f32,
    pub row_height: f32,
    pub combo_width: f32,
    pub action_gap: f32,
    pub keycap_height: f32,
    pub keycap_gap: f32,
    pub card_radius: f32,
    pub show_column_dividers: bool,
    pub show_resize_grip: bool,
    pub show_empty_message: bool,
}
```

Add `pub overlay_style: OverlayStyleSettings` to `AppSettings` and set it in `Default`.

Implement defaults matching current overlay values:

```rust
impl Default for RgbaColor {
    fn default() -> Self {
        Self { r: 0, g: 0, b: 0, a: 255 }
    }
}

impl RgbaColor {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl Default for OverlayStyleSettings {
    fn default() -> Self {
        Self {
            title_size: 18.0,
            subtitle_size: 12.0,
            group_heading_size: 13.0,
            action_text_size: 12.0,
            keycap_text_size: 9.5,
            card_background: RgbaColor::rgba(252, 251, 247, 242),
            card_border: RgbaColor::rgba(210, 210, 205, 170),
            title_color: RgbaColor::rgba(42, 42, 38, 255),
            group_heading_color: RgbaColor::rgba(34, 34, 31, 255),
            action_text_color: RgbaColor::rgba(52, 52, 48, 255),
            weak_text_color: RgbaColor::rgba(118, 116, 108, 255),
            divider_color: RgbaColor::rgba(202, 202, 196, 140),
            keycap_background: RgbaColor::rgba(246, 245, 241, 230),
            keycap_border: RgbaColor::rgba(170, 170, 164, 170),
            keycap_text_color: RgbaColor::rgba(44, 44, 40, 255),
            card_padding: 24.0,
            row_height: 18.0,
            combo_width: 112.0,
            action_gap: 8.0,
            keycap_height: 16.0,
            keycap_gap: 3.0,
            card_radius: 6.0,
            show_column_dividers: true,
            show_resize_grip: true,
            show_empty_message: true,
        }
    }
}
```

- [ ] **Step 4: Normalize overlay style**

Add `OverlayStyleSettings::normalize()` and call it from `AppSettings::normalize()`:

```rust
impl OverlayStyleSettings {
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        self.title_size = normalize_f32(self.title_size, 10.0, 36.0, defaults.title_size);
        self.subtitle_size = normalize_f32(self.subtitle_size, 8.0, 24.0, defaults.subtitle_size);
        self.group_heading_size = normalize_f32(self.group_heading_size, 8.0, 24.0, defaults.group_heading_size);
        self.action_text_size = normalize_f32(self.action_text_size, 8.0, 24.0, defaults.action_text_size);
        self.keycap_text_size = normalize_f32(self.keycap_text_size, 7.0, 18.0, defaults.keycap_text_size);
        self.card_padding = normalize_f32(self.card_padding, 0.0, 64.0, defaults.card_padding);
        self.keycap_height = normalize_f32(self.keycap_height, 10.0, 28.0, defaults.keycap_height);
        self.row_height = normalize_f32(self.row_height, 12.0, 40.0, defaults.row_height).max(self.keycap_height);
        self.combo_width = normalize_f32(self.combo_width, 64.0, 220.0, defaults.combo_width);
        self.action_gap = normalize_f32(self.action_gap, 0.0, 32.0, defaults.action_gap);
        self.keycap_gap = normalize_f32(self.keycap_gap, 0.0, 16.0, defaults.keycap_gap);
        self.card_radius = normalize_f32(self.card_radius, 0.0, 24.0, defaults.card_radius);
    }
}

fn normalize_f32(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() { value.clamp(min, max) } else { default }
}
```

- [ ] **Step 5: Run storage compatibility test**

Run: `cargo test --test app_settings app_settings_loads_default_overlay_style_when_missing_from_json`

Expected: PASS.

- [ ] **Step 6: Checkpoint changes**

Do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only the relevant files for this task.

Relevant files for this task: `src/storage.rs`, `tests/app_settings.rs`.

## Task 2: Storage Serialization And Normalization Tests

**Files:**
- Modify: `tests/app_settings.rs`
- Modify: `src/storage.rs`

- [ ] **Step 1: Write serialization test**

Update the existing import line in `tests/app_settings.rs` to include `OverlayStyleSettings` and `RgbaColor`, then add the test:

```rust
use cheatsheets::storage::{AppSettings, OverlayStyleSettings, RgbaColor, WindowPlacement};

#[test]
fn app_settings_serializes_overlay_style() {
    let settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            title_size: 22.0,
            card_background: RgbaColor::rgba(1, 2, 3, 200),
            show_column_dividers: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let value: serde_json::Value = serde_json::to_value(settings).unwrap();

    assert_eq!(value["overlay_style"]["title_size"], 22.0);
    assert_eq!(value["overlay_style"]["card_background"]["r"], 1);
    assert_eq!(value["overlay_style"]["card_background"]["a"], 200);
    assert_eq!(value["overlay_style"]["show_column_dividers"], false);
}
```

- [ ] **Step 2: Write normalization test**

Add:

```rust
#[test]
fn overlay_style_normalize_clamps_font_and_layout_values() {
    let mut settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            title_size: f32::NAN,
            subtitle_size: 99.0,
            group_heading_size: 1.0,
            action_text_size: 99.0,
            keycap_text_size: 1.0,
            card_padding: -1.0,
            row_height: 1.0,
            combo_width: 1.0,
            action_gap: 99.0,
            keycap_height: 28.0,
            keycap_gap: 99.0,
            card_radius: 99.0,
            ..Default::default()
        },
        ..Default::default()
    };

    settings.normalize();
    let style = settings.overlay_style;

    assert_eq!(style.title_size, OverlayStyleSettings::default().title_size);
    assert_eq!(style.subtitle_size, 24.0);
    assert_eq!(style.group_heading_size, 8.0);
    assert_eq!(style.action_text_size, 24.0);
    assert_eq!(style.keycap_text_size, 7.0);
    assert_eq!(style.card_padding, 0.0);
    assert_eq!(style.keycap_height, 28.0);
    assert_eq!(style.row_height, 28.0);
    assert_eq!(style.combo_width, 64.0);
    assert_eq!(style.action_gap, 32.0);
    assert_eq!(style.keycap_gap, 16.0);
    assert_eq!(style.card_radius, 24.0);
}
```

- [ ] **Step 3: Run tests and fix compilation**

Run: `cargo test --test app_settings`

Expected: PASS.

- [ ] **Step 4: Checkpoint changes**

Do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only the relevant files for this task.

Relevant files for this task: `src/storage.rs`, `tests/app_settings.rs`.

## Task 3: Resolve Overlay Style For Rendering

**Files:**
- Modify: `src/app.rs:6`
- Modify: `src/app.rs:657-820`
- Modify: `src/app.rs:1211-1216`
- Modify: `src/app.rs:1392-1571`

- [ ] **Step 1: Write failing default-resolution tests**

In `src/app.rs` test module, add:

```rust
#[test]
fn default_overlay_style_matches_current_visual_constants() {
    let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);

    assert_eq!(style.title_size, 18.0);
    assert_eq!(style.subtitle_size, 12.0);
    assert_eq!(style.group_heading_size, 13.0);
    assert_eq!(style.action_text_size, 12.0);
    assert_eq!(style.keycap_text_size, 9.5);
    assert_eq!(style.card_padding, 24.0);
    assert_eq!(style.row_height, 18.0);
    assert_eq!(style.combo_width, 112.0);
    assert_eq!(style.action_gap, 8.0);
    assert_eq!(style.keycap_height, 16.0);
    assert_eq!(style.keycap_gap, 3.0);
    assert_eq!(style.card_radius, 6.0);
}

#[test]
fn overlay_palette_scales_only_card_fill_and_border_by_opacity() {
    let style = OverlayStyleSettings::default();
    let low = resolved_overlay_style(&style, 0.55).palette;
    let high = resolved_overlay_style(&style, 1.0).palette;

    assert!(low.fill.a() < high.fill.a());
    assert!(low.border.a() < high.border.a());
    assert_eq!(low.text.a(), high.text.a());
    assert_eq!(low.divider.a(), high.divider.a());
    assert_eq!(low.keycap_fill.a(), high.keycap_fill.a());
    assert_eq!(low.keycap_border.a(), high.keycap_border.a());
    assert_eq!(low.keycap_text.a(), high.keycap_text.a());
}

#[test]
fn default_overlay_palette_matches_current_colors() {
    let palette = resolved_overlay_style(&OverlayStyleSettings::default(), 1.0).palette;

    assert_eq!(palette.fill, egui::Color32::from_rgba_unmultiplied(252, 251, 247, 242));
    assert_eq!(palette.border, egui::Color32::from_rgba_unmultiplied(210, 210, 205, 170));
    assert_eq!(palette.heading, egui::Color32::from_rgba_unmultiplied(42, 42, 38, 255));
    assert_eq!(palette.group_heading, egui::Color32::from_rgba_unmultiplied(34, 34, 31, 255));
    assert_eq!(palette.text, egui::Color32::from_rgba_unmultiplied(52, 52, 48, 255));
    assert_eq!(palette.weak_text, egui::Color32::from_rgba_unmultiplied(118, 116, 108, 255));
    assert_eq!(palette.divider, egui::Color32::from_rgba_unmultiplied(202, 202, 196, 140));
    assert_eq!(palette.keycap_fill, egui::Color32::from_rgba_unmultiplied(246, 245, 241, 230));
    assert_eq!(palette.keycap_border, egui::Color32::from_rgba_unmultiplied(170, 170, 164, 170));
    assert_eq!(palette.keycap_text, egui::Color32::from_rgba_unmultiplied(44, 44, 40, 255));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test default_overlay_style_matches_current_visual_constants
cargo test overlay_palette_scales_only_card_fill_and_border_by_opacity
cargo test default_overlay_palette_matches_current_colors
```

Expected: FAIL because `OverlayStyleSettings` is not imported and `resolved_overlay_style` does not exist.

- [ ] **Step 3: Import storage style types**

Change the storage import in `src/app.rs` to include `OverlayStyleSettings` and `RgbaColor`:

```rust
use crate::storage::{AppSettings, OverlayStyleSettings, RgbaColor, ThemeMode, WindowPlacement};
```

- [ ] **Step 4: Add resolved style type**

Near `ShortcutCardPalette`, add:

```rust
#[derive(Debug, Clone, Copy)]
struct ResolvedOverlayStyle {
    palette: ShortcutCardPalette,
    title_size: f32,
    subtitle_size: f32,
    group_heading_size: f32,
    action_text_size: f32,
    keycap_text_size: f32,
    card_padding: f32,
    row_height: f32,
    combo_width: f32,
    action_gap: f32,
    keycap_height: f32,
    keycap_gap: f32,
    card_radius: f32,
    show_column_dividers: bool,
    show_resize_grip: bool,
    show_empty_message: bool,
}

fn resolved_overlay_style(settings: &OverlayStyleSettings, opacity: f32) -> ResolvedOverlayStyle {
    let mut settings = settings.clone();
    settings.normalize();
    ResolvedOverlayStyle {
        palette: shortcut_card_palette(&settings, opacity),
        title_size: settings.title_size,
        subtitle_size: settings.subtitle_size,
        group_heading_size: settings.group_heading_size,
        action_text_size: settings.action_text_size,
        keycap_text_size: settings.keycap_text_size,
        card_padding: settings.card_padding,
        row_height: settings.row_height,
        combo_width: settings.combo_width,
        action_gap: settings.action_gap,
        keycap_height: settings.keycap_height,
        keycap_gap: settings.keycap_gap,
        card_radius: settings.card_radius,
        show_column_dividers: settings.show_column_dividers,
        show_resize_grip: settings.show_resize_grip,
        show_empty_message: settings.show_empty_message,
    }
}
```

- [ ] **Step 5: Convert RGBA colors to egui colors**

Add helpers:

```rust
fn rgba_to_color(color: RgbaColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn rgba_to_color_scaled_alpha(color: RgbaColor, opacity: f32) -> egui::Color32 {
    let alpha = (opacity.clamp(0.55, 1.0) * color.a as f32).round() as u8;
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, alpha)
}
```

- [ ] **Step 6: Update palette factory**

Replace `fn shortcut_card_palette(opacity: f32)` with:

```rust
fn shortcut_card_palette(settings: &OverlayStyleSettings, opacity: f32) -> ShortcutCardPalette {
    ShortcutCardPalette {
        fill: rgba_to_color_scaled_alpha(settings.card_background, opacity),
        border: rgba_to_color_scaled_alpha(settings.card_border, opacity),
        heading: rgba_to_color(settings.title_color),
        group_heading: rgba_to_color(settings.group_heading_color),
        text: rgba_to_color(settings.action_text_color),
        weak_text: rgba_to_color(settings.weak_text_color),
        divider: rgba_to_color(settings.divider_color),
        keycap_fill: rgba_to_color(settings.keycap_background),
        keycap_border: rgba_to_color(settings.keycap_border),
        keycap_text: rgba_to_color(settings.keycap_text_color),
    }
}
```

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test default_overlay_style_matches_current_visual_constants
cargo test overlay_palette_scales_only_card_fill_and_border_by_opacity
cargo test default_overlay_palette_matches_current_colors
```

Expected: PASS.

- [ ] **Step 8: Checkpoint changes**

Do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only the relevant files for this task.

Relevant files for this task: `src/app.rs`.

## Task 4: Apply Style To Overlay Rendering

**Files:**
- Modify: `src/app.rs:574-654`
- Modify: `src/app.rs:713-820`
- Modify: `src/app.rs:1025-1092`
- Modify: `src/app.rs:1211-1216`
- Modify: `src/app.rs:1392-1571`

- [ ] **Step 1: Write toggle tests**

Add private helpers and tests first. Tests:

```rust
#[test]
fn resize_grip_toggle_controls_resize_interaction() {
    assert!(should_show_resize_grip(&OverlayStyleSettings::default()));
    let style = OverlayStyleSettings { show_resize_grip: false, ..Default::default() };
    assert!(!should_show_resize_grip(&style));
}

#[test]
fn divider_toggle_controls_column_dividers() {
    assert!(should_show_column_dividers(&OverlayStyleSettings::default()));
    let style = OverlayStyleSettings { show_column_dividers: false, ..Default::default() };
    assert!(!should_show_column_dividers(&style));
}

#[test]
fn empty_message_toggle_controls_empty_state() {
    assert!(should_show_empty_message(&OverlayStyleSettings::default()));
    let style = OverlayStyleSettings { show_empty_message: false, ..Default::default() };
    assert!(!should_show_empty_message(&style));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test resize_grip_toggle_controls_resize_interaction
cargo test divider_toggle_controls_column_dividers
cargo test empty_message_toggle_controls_empty_state
```

Expected: FAIL because helpers do not exist.

- [ ] **Step 3: Add toggle helpers**

Add near overlay helper functions:

```rust
fn should_show_resize_grip(style: &OverlayStyleSettings) -> bool {
    style.show_resize_grip
}

fn should_show_column_dividers(style: &OverlayStyleSettings) -> bool {
    style.show_column_dividers
}

fn should_show_empty_message(style: &OverlayStyleSettings) -> bool {
    style.show_empty_message
}
```

- [ ] **Step 4: Update keycap helpers to accept style**

Change helper signatures and call sites:

```rust
fn keycap_width(label: &str) -> f32
fn combo_keycap_width(parts: &[&str], style: ResolvedOverlayStyle) -> f32
fn keycap_text_position(rect: egui::Rect) -> egui::Pos2
fn keycap_combo_start_x(rect_left: f32, rect_right: f32, parts: &[&str], style: ResolvedOverlayStyle) -> f32
fn show_keycap_combo(ui: &mut egui::Ui, rect: egui::Rect, combo: &str, style: ResolvedOverlayStyle)
```

Use `style.keycap_height`, `style.keycap_gap`, `style.keycap_text_size`, and `style.palette` inside `show_keycap_combo`.

- [ ] **Step 5: Update group heading font helper**

Replace `fn group_heading_font_id() -> egui::FontId` with:

```rust
fn group_heading_font_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from(KOREAN_BOLD_FONT_FAMILY)))
}
```

- [ ] **Step 6: Update overlay rendering to use resolved style**

In `show_shortcuts`, compute:

```rust
let style = resolved_overlay_style(&self.settings.overlay_style, self.settings.opacity);
let palette = style.palette;
```

Use `style.card_radius` for `rect_filled` and `rect_stroke`, `style.card_padding` for content shrink, `style.title_size` for display name, `style.subtitle_size` for description, and `style.show_empty_message` before rendering the empty-state label.

Wrap resize grip code in:

```rust
let grip_rect = resize_grip_rect(card_rect);
let resize_response = if style.show_resize_grip {
    Some(show_resize_grip(ui, grip_rect, palette))
} else {
    None
};
```

Use `resize_response.as_ref().is_some_and(...)` in drag exclusion checks. Also gate `pointer_in_resize_grip` by `style.show_resize_grip`; when the grip is disabled, bottom-right pointer positions must not block card drag-to-move.

Pass `style` to `show_shortcut_columns`.

- [ ] **Step 7: Update columns to use resolved style**

Change `show_shortcut_columns(ui, shortcuts, palette)` to `show_shortcut_columns(ui, shortcuts, style)`. Use:

```rust
let palette = style.palette;
if style.show_column_dividers { ... }
column.label(egui::RichText::new(group).font(group_heading_font_id(style.group_heading_size))...)
ui.set_min_height(style.row_height)
ui.allocate_exact_size(egui::vec2(style.combo_width, style.row_height), ...)
ui.add_space(style.action_gap)
egui::RichText::new(&entry.action).size(style.action_text_size)
```

- [ ] **Step 8: Update existing tests for new signatures**

Adjust tests that call `shortcut_card_palette`, `keycap_combo_start_x`, `combo_keycap_width`, and `group_heading_font_id` to use `OverlayStyleSettings::default()` and `resolved_overlay_style`.

- [ ] **Step 9: Run app tests**

Run: `cargo test --lib app::tests`

Expected: PASS.

- [ ] **Step 10: Checkpoint changes**

Do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only the relevant files for this task.

Relevant files for this task: `src/app.rs`.

## Task 5: Settings Menu Controls

**Files:**
- Modify: `src/app.rs:406-534`
- Modify: `src/app.rs:977-1023`

- [ ] **Step 1: Add color editing helpers**

Add helper functions near settings helpers:

```rust
fn rgba_color_edit(ui: &mut egui::Ui, label: &str, color: &mut RgbaColor) -> bool {
    let mut egui_color = rgba_to_color(*color);
    let changed = ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_srgba(&mut egui_color).changed()
    }).inner;
    if changed {
        *color = RgbaColor::rgba(egui_color.r(), egui_color.g(), egui_color.b(), egui_color.a());
    }
    changed
}

fn overlay_style_slider(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed |= ui.add(egui::Slider::new(value, range).suffix(suffix)).changed();
    });
    changed
}
```

- [ ] **Step 2: Add `오버레이 표시` settings section**

After the existing `화면` section in `show_settings`, insert a section:

```rust
ui.add_space(16.0);
settings_section(ui, "오버레이 표시", palette, |ui| {
    let mut changed = false;
    changed |= overlay_style_slider(ui, "제목 크기", &mut self.settings.overlay_style.title_size, 10.0..=36.0, "px");
    changed |= overlay_style_slider(ui, "설명 크기", &mut self.settings.overlay_style.subtitle_size, 8.0..=24.0, "px");
    changed |= overlay_style_slider(ui, "그룹 크기", &mut self.settings.overlay_style.group_heading_size, 8.0..=24.0, "px");
    changed |= overlay_style_slider(ui, "동작 크기", &mut self.settings.overlay_style.action_text_size, 8.0..=24.0, "px");
    changed |= overlay_style_slider(ui, "키캡 글자", &mut self.settings.overlay_style.keycap_text_size, 7.0..=18.0, "px");
    ui.separator();
    changed |= rgba_color_edit(ui, "카드", &mut self.settings.overlay_style.card_background);
    changed |= rgba_color_edit(ui, "카드 테두리", &mut self.settings.overlay_style.card_border);
    changed |= rgba_color_edit(ui, "제목", &mut self.settings.overlay_style.title_color);
    changed |= rgba_color_edit(ui, "본문", &mut self.settings.overlay_style.action_text_color);
    changed |= rgba_color_edit(ui, "보조", &mut self.settings.overlay_style.weak_text_color);
    changed |= rgba_color_edit(ui, "키캡 배경", &mut self.settings.overlay_style.keycap_background);
    changed |= rgba_color_edit(ui, "키캡 글자", &mut self.settings.overlay_style.keycap_text_color);
    ui.separator();
    changed |= overlay_style_slider(ui, "안쪽 여백", &mut self.settings.overlay_style.card_padding, 0.0..=64.0, "px");
    changed |= overlay_style_slider(ui, "행 높이", &mut self.settings.overlay_style.row_height, 12.0..=40.0, "px");
    changed |= overlay_style_slider(ui, "키캡 높이", &mut self.settings.overlay_style.keycap_height, 10.0..=28.0, "px");
    changed |= overlay_style_slider(ui, "모서리", &mut self.settings.overlay_style.card_radius, 0.0..=24.0, "px");
    changed |= ui.checkbox(&mut self.settings.overlay_style.show_column_dividers, "구분선 표시").changed();
    changed |= ui.checkbox(&mut self.settings.overlay_style.show_resize_grip, "리사이즈 그립 표시").changed();
    changed |= ui.checkbox(&mut self.settings.overlay_style.show_empty_message, "빈 메시지 표시").changed();
    if ui.button("기본값으로 되돌리기").clicked() {
        self.settings.overlay_style = OverlayStyleSettings::default();
        changed = true;
    }
    if changed {
        self.settings.overlay_style.normalize();
        self.persist_settings();
        ctx.request_repaint();
    }
});
```

- [ ] **Step 3: Compile-check the UI**

Run: `cargo check`

Expected: PASS.

- [ ] **Step 4: Checkpoint changes**

Do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only the relevant files for this task.

Relevant files for this task: `src/app.rs`.

## Task 6: Final Verification

**Files:**
- Modify only if verification finds issues.

- [ ] **Step 1: Format code**

Run: `cargo fmt`

Expected: command completes without errors.

- [ ] **Step 2: Check formatting**

Run: `cargo fmt -- --check`

Expected: PASS.

- [ ] **Step 3: Run full test suite**

Run: `cargo test`

Expected: PASS.

- [ ] **Step 4: Run compile check**

Run: `cargo check`

Expected: PASS.

- [ ] **Step 5: Manual smoke test**

Run: `cargo run`

Expected: app opens, settings menu shows `오버레이 표시`, controls persist, overlay updates after returning to shortcut view, resize grip disappears and stops resizing when disabled, reset restores defaults.

- [ ] **Step 6: Final checkpoint if verification changed files**

If formatting or fixes changed files after Task 5, do not commit unless the user explicitly requests it. If a commit is requested, first run `git status`, `git diff`, and `git log --oneline -5`, then stage only relevant implementation files.
