# Overlay Style Settings Design

## Goal

Let users configure the visual presentation of the shortcut overlay from the settings menu. The initial scope is global overlay styling: one style applies to every app overlay, matching the current settings model and avoiding per-app complexity.

## Current Context

- Overlay rendering lives in `src/app.rs`, primarily in `show_shortcuts`, `shortcut_card_palette`, and the shortcut column/keycap helpers.
- Persistent app settings live in `src/storage.rs` as `AppSettings` and are saved to `settings.json`.
- Existing settings already support theme, opacity, toggle hotkey, and window placement.
- The current overlay visual style is mostly hard-coded through `SHORTCUT_*` constants and `ShortcutCardPalette`.

## Approach

Add a nested global overlay style configuration to `AppSettings`, with defaults matching the current overlay as closely as possible. The settings menu will gain an `오버레이 표시` section that edits this configuration and persists changes through the existing settings save path.

Keep the rendering path in `src/app.rs`. Replace hard-coded visual constants and palette values with values resolved from the persisted overlay style. Keep storage/import/platform behavior unchanged.

## Settings Model

Add a serializable `OverlayStyleSettings` to `src/storage.rs` and include it in `AppSettings` as `overlay_style`.

The model should include these global options:

- Typography: title size, subtitle size, group heading size, action text size, keycap text size.
- Colors: card background, card border, title, group heading, action text, weak/subtitle text, divider, keycap background, keycap border, keycap text.
- Layout: card padding, row height, combo area width, action gap, keycap height, keycap gap, card radius.
- Visibility toggles: show column dividers, show resize grip, show empty-state message.

Represent colors as a small serializable RGBA type with `r`, `g`, `b`, and `a` `u8` fields. This preserves the current overlay defaults, where card fill, border, divider, and keycap surfaces use role-specific alpha values.

Opacity behavior is role-specific:

- Card fill alpha scales by the existing app opacity slider, matching the current `opacity * 242` behavior.
- Card border alpha scales by the existing app opacity slider, matching the current `opacity * 170` behavior.
- Text colors keep their configured alpha and do not scale with app opacity.
- Divider, keycap background, and keycap border keep their configured alpha and do not scale with app opacity.
- The existing app opacity slider remains the coarse card-opacity control; color alpha is the fine per-role default/power-user value.

`OverlayStyleSettings::default()` must reproduce current constants and palette colors. `normalize()` should clamp unsafe or unreadable values:

- Font sizes remain in practical ranges, for example `8.0..=32.0`.
- Layout dimensions remain finite and usable, for example padding and gaps are non-negative, row height is at least the keycap height, and combo width is wide enough for common shortcuts.
- Card radius stays non-negative.

Use exact normalization ranges so storage stays predictable:

- `title_size`: `10.0..=36.0`, default `18.0`.
- `subtitle_size`: `8.0..=24.0`, default `12.0`.
- `group_heading_size`: `8.0..=24.0`, default `13.0`.
- `action_text_size`: `8.0..=24.0`, default `12.0`.
- `keycap_text_size`: `7.0..=18.0`, default `9.5`.
- `card_padding`: `0.0..=64.0`, default `24.0`.
- `row_height`: `12.0..=40.0`, default `18.0`, then raised to at least `keycap_height`.
- `combo_width`: `64.0..=220.0`, default `112.0`.
- `action_gap`: `0.0..=32.0`, default `8.0`.
- `keycap_height`: `10.0..=28.0`, default `16.0`.
- `keycap_gap`: `0.0..=16.0`, default `3.0`.
- `card_radius`: `0.0..=24.0`, default `6.0`.

Use `#[serde(default)]` so older `settings.json` files load without migration code. Non-finite float values should be replaced with the corresponding default before clamping.

## Settings UI

Add an `오버레이 표시` section in `show_settings` after the existing `화면` section. The section should be compact and avoid overwhelming users.

Controls:

- Sliders for font sizes: 제목, 설명, 그룹, 동작, 키캡.
- Color pickers for major colors: 카드, 카드 테두리, 제목, 본문, 보조, 키캡 배경, 키캡 글자.
- Sliders for layout: 안쪽 여백, 행 높이, 키캡 높이, 모서리 둥글기.
- Checkboxes for 구분선 표시, 리사이즈 그립 표시, 빈 메시지 표시.
- A `기본값으로 되돌리기` button that resets only overlay style settings.

Persist changes immediately, consistent with existing theme/opacity behavior. Request repaint after changes.

## Rendering

Introduce a lightweight resolved style type in `src/app.rs` that converts `OverlayStyleSettings` into egui-ready values:

- `ShortcutCardPalette` should be built from overlay colors plus the existing opacity setting for alpha.
- Existing constants such as title size, subtitle size, group heading size, action text size, keycap text size, row height, combo width, action gap, keycap height, keycap gap, card padding, card radius, and palette colors should be replaced with resolved style fields where they affect user-visible overlay elements.
- Group heading font should accept the configured size while keeping the existing Korean bold font family.
- The overlay should still use the bright card style by default, independent of the app theme.

Keep behavior unchanged unless a setting explicitly changes it:

- Window chrome remains frameless for shortcut overlay.
- Drag-to-move remains available.
- Bottom-right resizing remains available by default. If `show_resize_grip` is disabled, both the painted resize grip and its drag interaction are disabled, so the overlay can only be resized again after re-enabling the option or changing saved window placement externally.
- The existing opacity slider continues to control overlay/card alpha.

## Testing

Add or update tests for:

- `AppSettings` loads old JSON without `overlay_style` and gets default overlay style.
- Overlay style values serialize to JSON.
- `normalize()` clamps invalid font and layout values.
- Default overlay style matches every current user-visible overlay styling constant and palette role listed in this spec: all font sizes, card padding, row height, combo width, action gap, keycap height, keycap gap, card radius, and all overlay palette colors/alpha values.
- Palette resolution applies app opacity to card fill and card border alpha, and does not apply app opacity to text, divider, keycap background, keycap border, or keycap text alpha.
- Toggle helpers correctly hide dividers, resize grip, and empty-state message when disabled.

Run:

- `cargo fmt -- --check`
- `cargo test`

## Acceptance Criteria

- The settings menu exposes global controls for overlay typography, colors, layout, and useful visibility toggles.
- Changing a setting immediately updates the overlay and persists to `settings.json`.
- Existing settings files without overlay style continue to load.
- Default overlay appearance remains visually equivalent to the current overlay, including all default values listed in the settings model.
- Users can reset overlay style settings to defaults without changing hotkey, theme, opacity, shortcuts, or window placement.
- Disabling the resize grip removes both the visual grip and bottom-right resize interaction.
- Existing shortcut import, app sheet metadata, hotkey, tray, and window placement behavior remain unchanged.
