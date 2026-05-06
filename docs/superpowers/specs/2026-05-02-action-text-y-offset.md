# Action Text Y Offset Setting

## Goal

Allow users to adjust only the shortcut action text vertical offset from the overlay display settings. This fixes optical mismatch where action text can appear lower than the keycap text.

## Requirements

- Add an `action_text_y_offset: f32` setting to `OverlayStyleSettings`.
- Apply the offset only to action text rendering.
- Keep keycap text positioning unchanged.
- Default value should preserve the current corrected visual alignment.
- Add the setting to `오버레이 표시 > 레이아웃` as `동작 세로 보정` with `px` suffix.
- Normalize the value to a small practical range: `-6.0..=6.0`.
- Preserve serde backward compatibility through existing `#[serde(default)]` behavior.
- Existing overlay style reset should reset this value to default.

## Design

`OverlayStyleSettings` stores the user-controlled value. `ResolvedOverlayStyle` carries it to overlay rendering. `shortcut_action_text_position` receives the offset and applies it to the action text y coordinate. `keycap_text_position` continues using the existing fixed keycap optical offset.

Default `action_text_y_offset` should be `-0.75`, matching the current hard-coded correction used after aligning action text to the keycap optical center.

## Tests

- Default overlay style includes `action_text_y_offset == -0.75`.
- Normalization clamps `action_text_y_offset` to `-6.0..=6.0`.
- Resolved overlay style carries the setting.
- Action text position changes by the configured offset.
- Keycap text position remains unchanged.
