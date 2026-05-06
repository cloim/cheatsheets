# Settings Popup Layout Redesign

## Goal

Improve the settings popup layout so settings fit the window better, use available space effectively, and feel easier to scan and edit. The visual direction should be close to the provided dark settings screenshot, but adapted to the app's existing theme palette.

This is a layout and UX refinement. It should not change settings semantics, persistence, overlay behavior, or the separate settings popup lifecycle.

## User Requirements

- Match the reference screenshot as much as practical while respecting the current theme.
- Focus on layout quality, not bottom navigation.
- Do not add previous/next footer navigation.
- Fix current UX issues where settings overflow the window, waste horizontal space, or appear as a long low-density list.
- Keep the existing settings sections and functionality.

## Current Context

- Settings already render in a separate egui child viewport with normal window chrome.
- The popup currently uses a simple horizontal split: narrow sidebar, separator, and a scrollable right panel.
- `오버레이 표시` is currently a long vertical list of sliders, color editors, checkboxes, and a reset button.
- This makes the most important settings exceed the visible area and does not use the popup width well.
- Existing helpers in `src/app.rs` include section-specific renderers and shared controls such as `rgba_color_edit`, `overlay_style_slider`, `shortcut_capture_button`, and `settings_section`.

## Design Direction

Use a three-level structure:

1. Fixed-width left sidebar for navigation.
2. Header area for the current section title, short description, and section actions.
3. Responsive content grid made of cards.

The popup should feel like a compact preferences window rather than a plain form. The dark theme should use layered navy/charcoal panels, subtle borders, rounded cards, and a blue selected-sidebar state similar to the reference. The light theme should keep the same layout but use the existing light palette rather than forcing dark colors.

## Sidebar

- Keep the sidebar fixed width, around `190-220px`.
- Show the settings title at the top.
- Show the section list below with clear selected state.
- Use the current section order: `일반`, `오버레이 표시`, `단축키`, `단축키 편집`.
- The selected item should be visually stronger than the current plain `selectable_value`, using a filled rounded row and accent stroke where feasible.
- Do not add bottom navigation.

Icons are optional. If they add implementation risk or require new assets, omit them. Layout and scanability matter more than icon parity.

## Header

Each section should have a header above its content:

- Section title.
- One-line description where helpful.
- Section-specific actions aligned right.

For `오버레이 표시`, place `기본값으로 되돌리기` in the header action area. This removes it from the bottom of a long form and makes the action discoverable without scrolling.

## Content Layout

The right panel should use available width with cards instead of one long vertical list.

At normal popup width, `오버레이 표시` should use a two-column card grid:

- Left card: `텍스트 크기` and `레이아웃`.
- Right card: `색상` and `표시 옵션`.

If the content width is too narrow, stack the cards vertically. The scroll area should belong to the right content panel only, not the entire window. The sidebar and header should remain stable.

Cards should:

- Have subtle background fill distinct from the page background.
- Have a thin border.
- Use consistent inner padding.
- Use section dividers inside a card only when they improve grouping.
- Avoid excessive vertical gaps.

## Overlay Display Controls

Replace the current full-width slider list with compact rows that align labels, controls, and units.

### Numeric Rows

Use compact numeric controls for size and layout values:

- Label on the left.
- Numeric input/control aligned in a consistent column.
- Unit such as `px` on the right.

The exact egui widget can remain `DragValue` or another native numeric input. Sliders are not required because the reference and UX goal favor dense numeric editing.

Text size rows:

- `제목 크기`
- `설명 크기`
- `그룹 크기`
- `동작 크기`
- `키캡 글자`

Layout rows:

- `안쪽 여백`
- `행 높이`
- `키 영역 너비`
- `동작 간격`
- `키캡 높이`
- `키캡 간격`
- `모서리`

### Color Rows

Use compact color rows in the right card:

- Label on the left.
- Small swatch button or compact color editor on the right.
- Keep the existing color data model and persistence.

Color rows:

- `카드`
- `카드 테두리`
- `제목`
- `그룹 제목`
- `본문`
- `보조`
- `구분선`
- `키캡 배경`
- `키캡 테두리`
- `키캡 글자`

If a custom dropdown color picker is too large for this pass, it is acceptable to keep egui's native color edit behavior inside a compact row, as long as the row layout is dense and visually aligned.

### Toggle Rows

Display options should use right-aligned toggles/check boxes in a compact list:

- `구분선 표시`
- `리사이즈 그립 표시`
- `빈 메시지 표시`

Use the existing boolean settings and immediate persistence behavior.

## Other Sections

`일반` should use one compact card for display settings:

- Theme selection.
- Opacity control.

`단축키` should use one compact card:

- Overlay toggle hotkey capture button.
- Short helper text if capture is active or empty.

`단축키 편집` should use clearer grouping:

- A compact editor card for combo/action/group/save.
- A separate area for import or existing shortcut list controls if present.
- Preserve current behavior and avoid broad refactoring unless required for layout.

## Theme Behavior

Use the existing theme resolution and palette as the source of truth.

- Dark theme: approximate the reference screenshot with deeper background, slightly lighter card fill, thin borders, and blue accent selection.
- Light theme: keep the same hierarchy but use light fills and existing accent colors.
- Default theme: follow the resolved system/app theme.

Do not hard-code the entire popup to dark mode.

## Data Flow

- Settings continue to edit `self.settings` directly.
- Changes continue to call normalization, persist immediately, and request repaint.
- Reset in the overlay header resets only `overlay_style` to defaults, then normalizes, persists, and repaints.
- No storage schema changes are needed.
- No changes are needed to tray actions, popup open/close behavior, root viewport visibility handling, or global hotkey capture semantics.

## Error Handling

- Existing persistence error behavior remains unchanged.
- If the popup is narrow, layout should degrade by stacking cards rather than clipping controls horizontally.
- If color controls require more space, prefer wrapping/stacking within the color card over forcing the whole window wider.

## Testing

Add or update tests only where behavior can be tested without fragile pixel assertions:

- `오버레이 표시` reset action remains available through the overlay section helper if testable through extracted logic.
- Section labels/order remain unchanged.
- Settings viewport minimum size remains large enough for the new layout.
- Existing settings persistence and overlay style normalization tests remain passing.

Manual verification is required for layout quality:

- Open settings in dark theme and verify the sidebar/header/card hierarchy resembles the reference.
- Verify `오버레이 표시` uses two columns at normal size and avoids a long single-column form.
- Resize narrower and verify cards stack or remain usable without horizontal clipping.
- Verify no bottom previous/next navigation exists.
- Verify reset, numeric edits, color edits, and toggles persist immediately.

Run:

- `cargo fmt -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo check`

## Acceptance Criteria

- The settings popup no longer presents `오버레이 표시` as one long low-density vertical list.
- The popup uses the available horizontal space with a card-based layout.
- The overlay display section fits substantially more controls in the initial viewport without sacrificing readability.
- Header actions, especially `기본값으로 되돌리기`, are visible without scrolling.
- The sidebar remains stable and visually indicates the selected section.
- There is no bottom previous/next navigation.
- Narrow window sizes remain usable through stacking or localized scrolling.
- Existing settings behavior, persistence, hotkey capture, shortcut editing, and popup lifecycle remain intact.
