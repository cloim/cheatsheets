# Overlay Layout Design

## Goal

Make the shortcut overlay visually match the user-provided Pixelmator-style reference more closely: a bright translucent card with compact shortcut groups, thin vertical dividers, and rows where keycap-style shortcut icons and descriptions align vertically.

## Approach

- Keep the existing `egui` rendering path in `src/app.rs` and avoid changing shortcut storage, grouping, import, or hotkey behavior.
- Render the shortcut view with a reference-style light card regardless of the configured theme: off-white fill, soft opacity, small corner radius, and a faint border.
- Present the app as a real overlay rather than a standard Windows window: remove the native title bar, resize border, and standard frame while keeping the transparent always-on-top viewport.
- Allow moving the overlay by dragging the internal card/background instead of using a native title bar.
- Allow resizing the frameless shortcut overlay with a subtle bottom-right resize grip. The resize should respect the existing minimum window size and persist through the existing window placement settings.
- The shortcut card itself should occupy the overlay viewport, with no transparent outer margin that can look like a black border on dark backgrounds.
- Use a large inset card with roughly `42px` outer margin, `24px` inner padding, and faint vertical separators between shortcut columns.
- Use compact typography: group headings around `13px`, shortcut rows around `12px`, and row height around `18px`.
- In each shortcut row, split the combo on `+` and render each part as a small rounded keycap badge, such as `Ctrl`, `Shift`, and `P`.
- Right-align the keycap badge run in a fixed-width combo area of roughly `112px` and left-align the action in the remaining text area so every row forms two clean vertical rails.
- Keep responsive behavior but use denser thresholds: prefer 4 columns at wide widths, 3 at medium widths, 2 at tablet widths, and 1 at narrow widths.

## Acceptance Criteria

- Modifier/key combinations such as `Ctrl+Shift+P` and single keys such as `F5` are shown as keycap badges and share the same right edge inside each group.
- Shortcut descriptions start at the same x-position inside each group.
- Shortcut descriptions are left-aligned within their action area, not centered.
- Columns are separated by subtle vertical lines when two or more columns are shown.
- The shortcut overlay uses the bright card treatment even when the app is configured for dark mode.
- The OS-level window chrome is not visible in normal overlay mode.
- The shortcut overlay does not show a black rectangular margin around the card.
- The shortcut overlay can be resized from its bottom-right grip without also triggering card move-drag.
- Settings view behavior and storage/import logic remain unchanged.

## Testing

- Run `cargo fmt -- --check`.
- Run `cargo test`.
- Run `cargo check` if the layout changes need compile-only validation before full tests.
