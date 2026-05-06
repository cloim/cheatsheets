# Settings Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move settings from the overlay viewport into a separate normal settings popup with section navigation.

**Architecture:** Keep the root viewport as the shortcut overlay host and render settings through a stable egui child viewport. Replace `AppView::Settings` with explicit settings popup state and split the settings page into section-specific helpers.

**Tech Stack:** Rust, eframe/egui 0.34, serde settings already in place, cargo test.

---

## File Structure

- Modify `src/app.rs`: add settings popup state, child viewport rendering, section enum/helpers, tray handling changes, tests.
- Modify `src/main.rs`: keep root viewport overlay-only and update tests only if helper expectations change.
- No storage changes expected for this feature.
- Existing overlay style settings work remains in the same feature branch and must stay intact.

## Task 1: Settings Popup State And Section Model

**Files:**
- Modify: `src/app.rs:54-87`
- Modify: `src/app.rs:116-138`
- Modify: `src/app.rs:1800-1942`

- [ ] **Step 1: Write failing tests for settings section defaults and labels**

Add to the `#[cfg(test)] mod tests` in `src/app.rs`:

```rust
#[test]
fn settings_section_defaults_to_general() {
    assert_eq!(SettingsSection::default(), SettingsSection::General);
}

#[test]
fn settings_section_labels_are_stable() {
    let labels = settings_sections()
        .iter()
        .map(|section| section.label())
        .collect::<Vec<_>>();

    assert_eq!(labels, vec!["일반", "오버레이 표시", "단축키", "단축키 편집"]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test settings_section_defaults_to_general
cargo test settings_section_labels_are_stable
```

Expected: FAIL because `SettingsSection` and `settings_sections` do not exist.

- [ ] **Step 3: Add `SettingsSection` enum and helpers**

In `src/app.rs`, near `AppView`, add:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    #[default]
    General,
    OverlayDisplay,
    Hotkeys,
    ShortcutEditor,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::General => "일반",
            Self::OverlayDisplay => "오버레이 표시",
            Self::Hotkeys => "단축키",
            Self::ShortcutEditor => "단축키 편집",
        }
    }
}

fn settings_sections() -> [SettingsSection; 4] {
    [
        SettingsSection::General,
        SettingsSection::OverlayDisplay,
        SettingsSection::Hotkeys,
        SettingsSection::ShortcutEditor,
    ]
}
```

- [ ] **Step 4: Add app fields**

Add fields to `CheatSheetsApp`:

```rust
settings_popup_open: bool,
settings_popup_needs_focus: bool,
settings_section: SettingsSection,
```

Initialize them in `CheatSheetsApp::new`:

```rust
settings_popup_open: false,
settings_popup_needs_focus: false,
settings_section: SettingsSection::default(),
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test settings_section_defaults_to_general
cargo test settings_section_labels_are_stable
```

Expected: PASS.

- [ ] **Step 6: Checkpoint changes**

Do not commit unless explicitly requested. If commit is requested, run `git status`, `git diff`, and `git log --oneline -5` first, then stage only relevant files.

## Task 2: Settings Viewport Configuration Helpers

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write failing viewport helper tests**

Add tests:

```rust
#[test]
fn settings_popup_viewport_uses_normal_window_chrome() {
    let viewport = settings_popup_viewport();

    assert_eq!(viewport.decorations, Some(true));
    assert_eq!(viewport.resizable, Some(true));
    assert_eq!(viewport.transparent, Some(false));
    assert_eq!(viewport.window_level, Some(egui::WindowLevel::AlwaysOnTop));
}

#[test]
fn settings_popup_uses_stable_viewport_id() {
    assert_eq!(settings_popup_viewport_id(), settings_popup_viewport_id());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test settings_popup_viewport_uses_normal_window_chrome
cargo test settings_popup_uses_stable_viewport_id
```

Expected: FAIL because helpers do not exist.

- [ ] **Step 3: Add viewport id and builder helpers**

Add near viewport/chrome helpers:

```rust
fn settings_popup_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("cheatsheets_settings_popup")
}

fn settings_popup_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("CheatSheets Settings")
        .with_inner_size([760.0, 620.0])
        .with_min_inner_size([560.0, 420.0])
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
}
```

If `with_window_level` is unavailable in egui 0.34, use the method used by `ViewportBuilder::with_always_on_top()` in `src/main.rs` or update the helper to the actual API after checking compiler output.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test settings_popup_viewport_uses_normal_window_chrome
cargo test settings_popup_uses_stable_viewport_id
```

Expected: PASS.

- [ ] **Step 5: Checkpoint changes**

Do not commit unless explicitly requested.

## Task 3: Tray Settings Action Opens Popup State

**Files:**
- Modify: `src/app.rs:240-250`
- Modify: `src/app.rs:1500-1942`

- [ ] **Step 1: Write state transition helper tests**

Add small pure helpers first through tests:

```rust
#[test]
fn settings_action_opens_popup_and_requests_focus() {
    let mut open = false;
    let mut needs_focus = false;

    open_settings_popup_state(&mut open, &mut needs_focus);

    assert!(open);
    assert!(needs_focus);
}

#[test]
fn overlay_hide_is_deferred_while_settings_popup_is_open() {
    assert!(!should_os_hide_root_overlay(true));
    assert!(should_os_hide_root_overlay(false));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test settings_action_opens_popup_and_requests_focus
cargo test overlay_hide_is_deferred_while_settings_popup_is_open
```

Expected: FAIL because helpers do not exist.

- [ ] **Step 3: Add pure state helpers**

Add:

```rust
fn open_settings_popup_state(open: &mut bool, needs_focus: &mut bool) {
    *open = true;
    *needs_focus = true;
}

fn should_os_hide_root_overlay(settings_popup_open: bool) -> bool {
    !settings_popup_open
}
```

- [ ] **Step 4: Update tray settings action**

Replace the current `TrayMenuAction::Settings` branch that switches to `AppView::Settings` with:

```rust
TrayMenuAction::Settings => {
    open_settings_popup_state(
        &mut self.settings_popup_open,
        &mut self.settings_popup_needs_focus,
    );
    self.visible = true;
    apply_viewport_chrome(ctx, self.view);
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.request_repaint();
}
```

Keep `self.visible = true` because egui 0.34 needs the root viewport visible for child viewport rendering.

- [ ] **Step 5: Update overlay hide command path**

In hotkey and Escape handling, when hiding the overlay, only send `ViewportCommand::Visible(false)` if `should_os_hide_root_overlay(self.settings_popup_open)` is true. If settings popup is open, set `self.visible = false`, keep the root OS-visible, set `self.settings_popup_needs_focus = true`, and request repaint.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test settings_action_opens_popup_and_requests_focus
cargo test overlay_hide_is_deferred_while_settings_popup_is_open
cargo test shortcuts_view_uses_overlay_chrome_but_settings_uses_window_chrome
```

Expected: PASS after updating the old chrome test in Task 4 if needed. If the old chrome test fails because `AppView::Settings` is removed, leave it failing until Task 4.

- [ ] **Step 7: Checkpoint changes**

Do not commit unless explicitly requested.

## Task 4: Settings Popup Rendering Shell

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add popup lifecycle tests**

Add pure helper tests if the logic is extracted:

```rust
#[test]
fn settings_popup_close_request_closes_only_popup() {
    let mut open = true;
    close_settings_popup_state(&mut open);
    assert!(!open);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test settings_popup_close_request_closes_only_popup`

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Add close helper**

Add:

```rust
fn close_settings_popup_state(open: &mut bool) {
    *open = false;
}
```

- [ ] **Step 4: Implement `show_settings_popup` shell**

Add method on `CheatSheetsApp`:

```rust
fn show_settings_popup(&mut self, ctx: &egui::Context, palette: UiPalette) {
    if !self.settings_popup_open {
        return;
    }
    let viewport_id = settings_popup_viewport_id();
    let viewport = settings_popup_viewport();
    ctx.show_viewport_immediate(viewport_id, viewport, |ui, _class| {
        let popup_ctx = ui.ctx().clone();
        if popup_ctx.input(|input| input.viewport().close_requested()) {
            close_settings_popup_state(&mut self.settings_popup_open);
            return;
        }
        if popup_ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            close_settings_popup_state(&mut self.settings_popup_open);
            return;
        }
        self.handle_shortcut_capture(&popup_ctx);
        configure_style(&popup_ctx, self.settings.theme);
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.show_settings_popup_contents(ui, &popup_ctx, palette);
        });
        if self.settings_popup_needs_focus {
            popup_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.settings_popup_needs_focus = false;
        }
    });
    if !self.settings_popup_open && !self.visible {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }
}
```

This uses the egui 0.34 `show_viewport_immediate` callback shape: `|ui, _class|`, not `|ctx, _class|`.

- [ ] **Step 5: Add focus helper for overlay repaints while settings is open**

When root overlay is shown/repainted while `self.settings_popup_open` is true, set `self.settings_popup_needs_focus = true` before requesting repaint. This should happen in the tray settings action and in the hotkey path when the overlay becomes visible while settings is already open.

- [ ] **Step 6: Run compile check**

Run: `cargo check`

Expected: PASS. If egui child viewport API names differ, fix to the actual egui 0.34 API and rerun.

- [ ] **Step 7: Checkpoint changes**

Do not commit unless explicitly requested.

## Task 5: Remove Main-Viewport Settings Mode

**Files:**
- Modify: `src/app.rs:54-70`
- Modify: `src/app.rs:406-656`
- Modify: `src/app.rs:1580-1620`
- Modify: `src/app.rs:1879-1894`

- [ ] **Step 1: Write/adjust tests for no reachable root settings mode**

Replace tests that assert settings uses root viewport chrome with tests that assert root chrome is always overlay chrome:

```rust
#[test]
fn root_viewport_chrome_stays_overlay_only() {
    assert_eq!(viewport_decorations_for_view(AppView::Shortcuts), false);
    assert_eq!(viewport_resizable_for_view(AppView::Shortcuts), false);
}
```

Remove assertions for `AppView::Settings` if the enum variant is removed.

- [ ] **Step 2: Run test to verify failure if old settings path remains**

Run: `cargo test root_viewport_chrome_stays_overlay_only`

Expected: PASS if helper already supports shortcuts; old tests may still fail until Step 3.

- [ ] **Step 3: Remove or isolate `AppView::Settings`**

Preferred minimal path:

- Change `AppView` to only `Shortcuts`, or remove `AppView` entirely if compiler changes are small.
- Remove `paints_full_window_background(AppView::Settings)` behavior.
- Remove the main `match self.view { AppView::Settings => ... }` rendering path.
- Keep `show_settings` temporarily but stop calling it from root; it will be split into popup helpers in Task 6.

- [ ] **Step 4: Update root UI flow**

Ensure root `ui` flow after style/palette setup:

```rust
self.show_settings_popup(&ctx, palette);
if !self.visible {
    return;
}
// render overlay shortcuts only
self.show_shortcuts(ui);
```

If `self.show_settings_popup` closes the popup and `self.visible` is false, it must send `ViewportCommand::Visible(false)` to hide the root after the child viewport has closed. Do not paint a full window background for settings on the root viewport.

- [ ] **Step 5: Run app tests**

Run: `cargo test --lib app::tests`

Expected: PASS after updating removed `AppView::Settings` tests.

- [ ] **Step 6: Checkpoint changes**

Do not commit unless explicitly requested.

## Task 6: Split Settings Into Sidebar Sections

**Files:**
- Modify: `src/app.rs:406-656`
- Modify: `src/app.rs:1200-1280`

- [ ] **Step 1: Implement settings popup contents and sidebar**

Add methods:

```rust
fn show_settings_popup_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, palette: UiPalette) {
    ui.horizontal(|ui| {
        ui.set_min_size(ui.available_size());
        ui.vertical(|ui| {
            ui.set_width(160.0);
            ui.heading("설정");
            ui.add_space(12.0);
            self.show_settings_sidebar(ui);
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.settings_section {
                SettingsSection::General => self.show_settings_general(ui, ctx, palette),
                SettingsSection::OverlayDisplay => self.show_settings_overlay_display(ui, ctx),
                SettingsSection::Hotkeys => self.show_settings_hotkeys(ui, palette),
                SettingsSection::ShortcutEditor => self.show_settings_shortcut_editor(ui, palette),
            }
        });
    });
}

fn show_settings_sidebar(&mut self, ui: &mut egui::Ui) {
    for section in settings_sections() {
        ui.selectable_value(&mut self.settings_section, section, section.label());
    }
}
```

- [ ] **Step 2: Split existing `show_settings` body into section helpers**

Move existing blocks without changing behavior:

- `화면` block -> `show_settings_general`
- `오버레이 표시` block -> `show_settings_overlay_display`
- `오버레이 단축키` block -> `show_settings_hotkeys`
- `단축키 추가 / 수정` block and status label -> `show_settings_shortcut_editor`

Keep `self.handle_shortcut_capture(ctx)` in the popup shell or in sections that need capture. Recommended: call once in `show_settings_popup` before rendering contents.

- [ ] **Step 3: Remove old root `show_settings` method**

After section helpers compile, delete the old monolithic `show_settings` method or leave it unused only temporarily. Final code should not have a dead `show_settings` warning.

- [ ] **Step 4: Run compile check**

Run: `cargo check`

Expected: PASS with no dead-code warnings for old settings path.

- [ ] **Step 5: Run app tests**

Run: `cargo test --lib app::tests`

Expected: PASS.

- [ ] **Step 6: Checkpoint changes**

Do not commit unless explicitly requested.

## Task 7: Full Verification And Manual Smoke

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

- [ ] **Step 5: Build release binary**

Run: `cargo build --release`

Expected: PASS.

- [ ] **Step 6: Manual smoke test**

Run the release binary and verify:

- Tray `설정` opens a separate decorated settings popup.
- Overlay remains present behind/alongside settings.
- Sidebar sections switch content.
- `오버레이 표시` settings still persist and update overlay.
- Closing settings popup leaves app running.
- Reopening settings focuses/reuses the popup.

- [ ] **Step 7: Final checkpoint**

Do not commit unless explicitly requested. If commit is requested, run `git status`, `git diff`, and `git log --oneline -5` before staging.
