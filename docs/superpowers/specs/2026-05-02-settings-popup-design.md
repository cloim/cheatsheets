# Settings Popup Design

## Goal

Move settings out of the shortcut overlay viewport and present them as a normal settings popup window with section navigation. The shortcut overlay should remain visible while settings are open.

## Current Context

- The app currently uses one `egui` viewport and switches between `AppView::Shortcuts` and `AppView::Settings`.
- Shortcut overlay mode is frameless, transparent, always-on-top, and manually draggable/resizable.
- Settings mode currently reuses the same viewport, temporarily enabling decorations/resizing and painting an opaque background.
- The settings UI in `show_settings` is a single vertical page containing screen settings, overlay display settings, hotkey settings, and shortcut editing.
- The current feature branch already adds overlay display settings to `AppSettings` and the settings page.

## Approach

Keep the main viewport dedicated to the shortcut overlay. Replace the in-place settings view with a separate settings viewport opened from the tray menu. The settings viewport should behave like a normal app preferences window: decorated, resizable, opaque, focused when opened, and independently closable.

Opening settings should not replace the overlay. The overlay host remains visible while settings are open so egui can keep submitting the settings child viewport. Closing the settings popup should only close that popup.

## Window Behavior

- Main viewport remains shortcut-overlay-only: frameless, transparent, and not OS-resizable.
- Tray `설정` opens a separate settings viewport rather than setting `AppView::Settings` on the main viewport.
- Opening settings makes the root overlay host visible if it was hidden. This is required because egui 0.34 child viewports are submitted from the root viewport frame. The settings popup receives focus and sits above the overlay.
- The settings viewport uses normal window chrome: `Decorations(true)`, `Resizable(true)`, opaque background, and a reasonable default size such as `760x620`.
- The settings viewport should request focus when opened.
- Because the overlay is always-on-top, the settings viewport should also use an always-on-top window level while it is open so it is not hidden behind the overlay. Whenever settings is opened or already open while the overlay repaints, send focus to the settings viewport after rendering/requesting the popup so it stays above the overlay in normal use.
- Pressing Escape in the main overlay still hides the overlay. Pressing Escape or using the window close control in the settings viewport closes only the settings popup.
- Multiple tray `설정` clicks should focus/reopen the same settings popup, not create duplicate settings windows.

For egui 0.34 child viewport lifecycle, the settings viewport must be submitted every frame while `settings_popup_open` is true. While settings is open, the root viewport must stay OS-visible even if the user toggles the overlay hotkey. Hotkey toggling may hide shortcut contents conceptually after the settings popup closes, but it must not send `ViewportCommand::Visible(false)` to the root while the settings popup is open.

## State Model

Replace the view-switching settings state with explicit settings popup state:

- Keep `AppView::Shortcuts` or remove `AppView` entirely if the code becomes clearer. Do not keep a reachable `AppView::Settings` path in the main viewport.
- Add a field such as `settings_popup_open: bool`.
- Add a field such as `settings_section: SettingsSection`.
- Add a fixed child viewport id such as `egui::ViewportId::from_hash_of("settings_popup")`. Reuse this id for every settings popup render and focus command.
- Add a small flag such as `settings_popup_needs_focus: bool` so tray clicks can request focus without creating another popup.

`SettingsSection` should be a small enum with these values:

- `General`
- `OverlayDisplay`
- `Hotkeys`
- `ShortcutEditor`

Default section should be `General`.

## Settings Layout

The settings popup uses a two-pane layout:

- Left sidebar: section selector buttons.
- Right panel: selected section contents.

Sections:

- `일반`: theme and opacity.
- `오버레이 표시`: title/subtitle/group/action/keycap font sizes, card/keycap/text colors, layout sliders, divider/resize grip/empty message toggles, reset-to-default.
- `단축키`: overlay toggle hotkey capture.
- `단축키 편집`: current active process, combo/action/group editor, save, and file import.

The right panel should be scrollable because overlay display settings can be tall. Use existing save behavior: changes persist immediately and request repaint.

## Rendering And Code Organization

Keep the implementation in `src/app.rs` for now to match the existing codebase, but split the settings UI into focused helpers:

- `show_settings_popup`
- `show_settings_sidebar`
- `show_settings_general`
- `show_settings_overlay_display`
- `show_settings_hotkeys`
- `show_settings_shortcut_editor`

Keep shared controls such as `rgba_color_edit`, `overlay_style_slider`, `shortcut_capture_button`, and `settings_section` reusable across section helpers.

The main `ui` function should render the settings popup separately through egui viewport/window APIs before returning for hidden overlay state. It should render shortcuts when `self.visible` is true. While settings is open, keep the root viewport visible so settings rendering continues.

Use egui child viewport APIs available in egui/eframe 0.34, such as `Context::show_viewport_immediate`, with a stable `ViewportId` and a `ViewportBuilder` configured for the settings popup. Inside the settings viewport callback:

- Detect `viewport().close_requested()` and set `settings_popup_open = false`.
- Close on Escape without changing root overlay visibility.
- Render the sidebar and selected section.
- If `settings_popup_needs_focus` is set, send a focus command to the settings viewport and clear the flag.

## Data Flow

- Tray menu sends `TrayMenuAction::Settings`.
- The app sets `settings_popup_open = true`, sets `settings_popup_needs_focus = true`, makes the root viewport visible, and requests repaint. It does not change the main overlay viewport chrome.
- Repeated tray settings actions keep `settings_popup_open = true`, set `settings_popup_needs_focus = true`, and reuse the same settings viewport id.
- Each settings section edits `self.settings` directly, calls `normalize()`, persists through `persist_settings()`, and requests repaint.
- Hotkey capture still works from the settings popup and reinstalls the global hotkey after capture.
- Shortcut editing still targets the current active app identity and uses existing catalog persistence/import logic.

## Testing

Add or update tests for:

- Settings no longer use the main overlay viewport chrome path.
- The main shortcut viewport remains frameless and not OS-resizable.
- Opening settings uses a decorated, resizable, opaque settings viewport configuration.
- Settings popup open action does not change the main view away from shortcuts.
- Opening settings makes the root viewport visible so the settings child viewport can be submitted.
- While settings is open, overlay hotkey hiding does not OS-hide the root viewport.
- Settings popup close request closes only the settings popup state.
- Repeated tray settings action reuses the same viewport id and requests focus instead of creating a duplicate.
- `SettingsSection` defaults to `General`.
- Section labels/order are stable: `일반`, `오버레이 표시`, `단축키`, `단축키 편집`.
- Escape handling distinguishes main overlay hiding from settings popup closing if helper logic is introduced.

Run:

- `cargo fmt -- --check`
- `cargo test`
- `cargo check`

## Acceptance Criteria

- Opening settings from the tray shows a separate normal settings popup window.
- The shortcut overlay remains visible while the settings popup is open.
- If the shortcut overlay is hidden, opening settings makes the root overlay host visible as required by egui child viewport lifecycle, then focuses the settings popup above it.
- The settings popup has a left section selector and right section-specific settings panel.
- Settings are grouped into `일반`, `오버레이 표시`, `단축키`, and `단축키 편집`.
- Settings changes persist immediately and update the overlay without closing the popup.
- Closing the settings popup does not close the app and does not hide the overlay.
- Reopening settings focuses/reuses the settings popup instead of creating duplicates.
- Existing shortcut overlay rendering, hotkey toggling, tray restart/close, custom shortcut import, and settings persistence behavior remain intact.
