use crate::core::{
    AppIdentity, Catalog, ShortcutEntry, ShortcutSource, UserShortcutPatch, normalize_app_id,
};
use crate::import::{ImportFormat, parse_shortcut_import};
use crate::storage::{AppSettings, ThemeMode, WindowPlacement};
use crate::{platform, storage};
use anyhow::{Context, Result, anyhow};
use eframe::egui;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{Arc, Mutex, mpsc},
};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

const APP_NAME: &str = "CheatSheet";
const TRAY_MENU_SETTINGS: &str = "cheatsheet.settings";
const TRAY_MENU_CLOSE: &str = "cheatsheet.close";
const KOREAN_FONT_REGULAR: &str = "malgun_gothic";
const KOREAN_FONT_BOLD: &str = "malgun_gothic_bold";
const KOREAN_BOLD_FONT_FAMILY: &str = "korean_bold";

pub struct CheatSheetApp {
    catalog: Catalog,
    active: AppIdentity,
    visible: bool,
    hotkey: Option<HotKey>,
    hotkey_manager: Option<GlobalHotKeyManager>,
    hotkey_rx: mpsc::Receiver<GlobalHotKeyEvent>,
    hotkey_tx: mpsc::Sender<GlobalHotKeyEvent>,
    repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    tray_icon: Option<TrayIcon>,
    tray_rx: mpsc::Receiver<TrayMenuAction>,
    custom_index: storage::UserCatalogIndex,
    loaded_custom_apps: BTreeSet<String>,
    settings: AppSettings,
    view: AppView,
    capture_target: Option<CaptureTarget>,
    status: String,
    draft_combo: String,
    draft_action: String,
    draft_group: String,
    last_persisted_window: Option<WindowPlacement>,
    last_window_persist_time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppView {
    CheatSheet,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureTarget {
    ToggleHotkey,
    ShortcutCombo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMenuAction {
    Settings,
    Close,
}

impl CheatSheetApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let catalog = Catalog::with_builtins();
        let (custom_index, mut status) = match storage::load_user_catalog_index() {
            Ok(custom_index) => (custom_index, String::new()),
            Err(error) => (
                storage::UserCatalogIndex::default(),
                format!("사용자 정의 파일 목록을 읽지 못했습니다: {error:#}"),
            ),
        };
        if let Err(error) = install_korean_font(&cc.egui_ctx) {
            append_status(&mut status, format!("한글 글꼴 로드 실패: {error:#}"));
        }
        let settings = match storage::load_app_settings() {
            Ok(settings) => settings,
            Err(error) => {
                append_status(&mut status, format!("설정 로드 실패: {error:#}"));
                AppSettings::default()
            }
        };
        configure_style(&cc.egui_ctx, settings.theme);
        let last_persisted_window = settings.window;

        let (hotkey_tx, hotkey_rx) = mpsc::channel();
        let (tray_tx, tray_rx) = mpsc::channel();
        let repaint_ctx = Arc::new(Mutex::new(None));

        let mut app = Self {
            catalog,
            active: platform::active_window().unwrap_or_else(AppIdentity::unknown),
            visible: true,
            hotkey: None,
            hotkey_manager: None,
            hotkey_rx,
            hotkey_tx: hotkey_tx.clone(),
            repaint_ctx: Arc::clone(&repaint_ctx),
            tray_icon: None,
            tray_rx,
            custom_index,
            loaded_custom_apps: BTreeSet::new(),
            settings,
            view: AppView::CheatSheet,
            capture_target: None,
            status,
            draft_combo: String::new(),
            draft_action: String::new(),
            draft_group: "Custom".to_owned(),
            last_persisted_window,
            last_window_persist_time: -1.0,
        };
        app.install_hotkey(hotkey_tx, repaint_ctx);
        app.install_tray(tray_tx);
        app
    }

    fn install_hotkey(
        &mut self,
        sender: mpsc::Sender<GlobalHotKeyEvent>,
        repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    ) {
        let hotkey = match parse_hotkey_for_registration(&self.settings.toggle_hotkey) {
            Ok(hotkey) => hotkey,
            Err(error) => {
                self.status = format!("전역 단축키 등록 실패: {error}");
                return;
            }
        };

        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => manager,
            Err(error) => {
                self.status = format!("전역 단축키를 사용할 수 없습니다: {error}");
                return;
            }
        };

        if let Err(error) = manager.register(hotkey) {
            self.status = format!("전역 단축키 등록 실패: {error}");
            return;
        }

        GlobalHotKeyEvent::set_event_handler(Some(move |event| {
            let _ = sender.send(event);
            if let Ok(guard) = repaint_ctx.lock()
                && let Some(ctx) = guard.as_ref()
            {
                ctx.request_repaint();
            }
        }));

        self.hotkey = Some(hotkey);
        self.hotkey_manager = Some(manager);
    }

    fn reinstall_hotkey(&mut self) {
        if let (Some(manager), Some(hotkey)) = (&self.hotkey_manager, self.hotkey) {
            let _ = manager.unregister(hotkey);
        }
        self.hotkey = None;
        self.hotkey_manager = None;
        self.install_hotkey(self.hotkey_tx.clone(), Arc::clone(&self.repaint_ctx));
    }

    fn install_tray(&mut self, sender: mpsc::Sender<TrayMenuAction>) {
        match build_tray_icon() {
            Ok(tray_icon) => {
                let repaint_ctx = Arc::clone(&self.repaint_ctx);
                MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                    let action = match event.id.as_ref() {
                        TRAY_MENU_SETTINGS => Some(TrayMenuAction::Settings),
                        TRAY_MENU_CLOSE => Some(TrayMenuAction::Close),
                        _ => None,
                    };
                    if let Some(action) = action {
                        let _ = sender.send(action);
                        if let Ok(guard) = repaint_ctx.lock()
                            && let Some(ctx) = guard.as_ref()
                        {
                            ctx.request_repaint();
                        }
                    }
                }));
                self.tray_icon = Some(tray_icon);
            }
            Err(error) => {
                append_status(
                    &mut self.status,
                    format!("트레이 아이콘 생성 실패: {error:#}"),
                );
            }
        }
    }

    fn remember_repaint_context(&self, ctx: &egui::Context) {
        if let Ok(mut repaint_ctx) = self.repaint_ctx.lock() {
            *repaint_ctx = Some(ctx.clone());
        }
    }

    fn poll_hotkey(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.hotkey_rx.try_recv() {
            if is_toggle_event(self.hotkey, event) {
                let will_show = !self.visible;
                if will_show {
                    self.active = platform::active_window().unwrap_or_else(AppIdentity::unknown);
                    self.view = AppView::CheatSheet;
                }
                self.visible = will_show;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.visible));
                ctx.request_repaint();
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayMenuAction::Settings => {
                    self.view = AppView::Settings;
                    self.visible = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
                TrayMenuAction::Close => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn add_or_replace_user_shortcut(&mut self) {
        if self.draft_combo.trim().is_empty() || self.draft_action.trim().is_empty() {
            self.status = "단축키와 동작은 필수입니다.".to_owned();
            return;
        }

        let active_app_id = normalize_app_id(&self.active.app_id);
        if let Err(error) = self.ensure_user_shortcuts_loaded(&active_app_id) {
            self.status = format!("기존 사용자 정의 단축키를 읽지 못했습니다: {error:#}");
            return;
        }

        let entry = ShortcutEntry::new(
            self.draft_combo.trim(),
            self.draft_action.trim(),
            self.draft_group.trim(),
            ShortcutSource::User,
        );
        self.catalog
            .add_user_patch(active_app_id, UserShortcutPatch::replace(entry));
        self.persist_user_catalog();
        self.draft_combo.clear();
        self.draft_action.clear();
    }

    fn persist_user_catalog(&mut self) {
        match storage::save_user_catalog(&self.catalog.user_catalog()) {
            Ok(path) => self.status = format!("저장했습니다: {}", path.display()),
            Err(error) => self.status = format!("저장 실패: {error:#}"),
        }
    }

    fn persist_settings(&mut self) {
        self.settings.normalize();
        match storage::save_app_settings(&self.settings) {
            Ok(path) => self.status = format!("설정을 저장했습니다: {}", path.display()),
            Err(error) => self.status = format!("설정 저장 실패: {error:#}"),
        }
    }

    fn persist_window_settings_if_changed(&mut self, ctx: &egui::Context) {
        let Some(window) = current_window_placement(ctx) else {
            return;
        };
        if !self
            .settings
            .window
            .is_some_and(|saved| !window.differs_from(&saved))
        {
            self.settings.window = Some(window);
        }

        if self
            .last_persisted_window
            .is_some_and(|saved| !window.differs_from(&saved))
        {
            return;
        }

        let now = ctx.input(|input| input.time);
        if now - self.last_window_persist_time < 0.75 {
            return;
        }

        let mut settings = self.settings.clone();
        settings.window = Some(window);
        settings.normalize();
        if storage::save_app_settings(&settings).is_ok() {
            self.settings = settings;
            self.last_persisted_window = Some(window);
            self.last_window_persist_time = now;
        }
    }

    fn import_shortcuts_from_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Shortcut files", &["json", "csv"])
            .add_filter("JSON", &["json"])
            .add_filter("CSV", &["csv"])
            .pick_file()
        else {
            return;
        };

        let result = (|| -> Result<usize> {
            let format = ImportFormat::from_path(&path)?;
            let content = fs::read_to_string(&path)
                .with_context(|| format!("파일을 읽지 못했습니다: {}", path.display()))?;
            let import = parse_shortcut_import(&content, format, &self.active.app_id)?;
            let imported_count = import.imported_count;
            for app_id in import.catalog.apps.keys() {
                self.ensure_user_shortcuts_loaded(app_id)?;
            }
            self.catalog.merge_user_catalog(import.catalog);
            storage::save_user_catalog(&self.catalog.user_catalog())?;
            Ok(imported_count)
        })();

        match result {
            Ok(count) => {
                self.status = format!("단축키 {count}개를 가져왔습니다: {}", path.display());
            }
            Err(error) => {
                self.status = format!("가져오기 실패: {error:#}");
            }
        }
    }

    fn handle_shortcut_capture(&mut self, ctx: &egui::Context) {
        let Some(target) = self.capture_target else {
            return;
        };
        if let Some(combo) = capture_combo_from_events(ctx) {
            match target {
                CaptureTarget::ToggleHotkey => {
                    self.settings.toggle_hotkey = combo;
                    self.persist_settings();
                    self.reinstall_hotkey();
                }
                CaptureTarget::ShortcutCombo => {
                    self.draft_combo = combo;
                }
            }
            self.capture_target = None;
            ctx.request_repaint();
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, palette: UiPalette) {
        self.handle_shortcut_capture(ctx);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("설정").color(palette.heading));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("오버레이").clicked() {
                    self.view = AppView::CheatSheet;
                }
            });
        });
        ui.add_space(18.0);

        settings_section(ui, "화면", palette, |ui| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("테마");
                changed |= ui
                    .selectable_value(&mut self.settings.theme, ThemeMode::Default, "기본")
                    .changed();
                changed |= ui
                    .selectable_value(&mut self.settings.theme, ThemeMode::Light, "Light")
                    .changed();
                changed |= ui
                    .selectable_value(&mut self.settings.theme, ThemeMode::Dark, "Dark")
                    .changed();
            });

            ui.horizontal(|ui| {
                ui.label("투명도");
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.opacity, 0.55..=1.0).show_value(false),
                    )
                    .changed();
                ui.monospace(format!("{:.0}%", self.settings.opacity * 100.0));
            });

            if changed {
                self.settings.normalize();
                configure_style(ctx, self.settings.theme);
                self.persist_settings();
                ctx.request_repaint();
            }
        });

        ui.add_space(16.0);
        settings_section(ui, "오버레이 단축키", palette, |ui| {
            ui.horizontal(|ui| {
                ui.label("토글");
                if shortcut_capture_button(
                    ui,
                    &self.settings.toggle_hotkey,
                    self.capture_target == Some(CaptureTarget::ToggleHotkey),
                    palette,
                )
                .clicked()
                {
                    self.capture_target = Some(CaptureTarget::ToggleHotkey);
                }
            });
        });

        ui.add_space(16.0);
        settings_section(ui, "단축키 추가 / 수정", palette, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "대상 프로세스: {}",
                    normalize_app_id(&self.active.app_id)
                ))
                .color(palette.weak_text),
            );
            ui.add_space(6.0);

            egui::Grid::new("shortcut_settings_editor")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Combo");
                    if shortcut_capture_button(
                        ui,
                        &self.draft_combo,
                        self.capture_target == Some(CaptureTarget::ShortcutCombo),
                        palette,
                    )
                    .clicked()
                    {
                        self.capture_target = Some(CaptureTarget::ShortcutCombo);
                    }
                    ui.end_row();

                    ui.label("Action");
                    ui.add(borderless_text_edit(&mut self.draft_action).desired_width(420.0));
                    ui.end_row();

                    ui.label("Group");
                    ui.horizontal(|ui| {
                        ui.add(borderless_text_edit(&mut self.draft_group).desired_width(220.0));
                        if ui.button("저장").clicked() {
                            self.add_or_replace_user_shortcut();
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button("파일 가져오기")
                    .on_hover_text(
                        "CSV는 현재 대상 프로세스에, JSON은 파일의 apps 구조대로 병합합니다.",
                    )
                    .clicked()
                {
                    self.import_shortcuts_from_file();
                }
            });
        });

        if !self.status.is_empty() {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(&self.status)
                    .size(12.0)
                    .color(palette.weak_text),
            );
        }
    }

    fn ensure_user_shortcuts_loaded(&mut self, app_id: &str) -> Result<()> {
        let app_id = normalize_app_id(app_id);
        if self.loaded_custom_apps.contains(&app_id) {
            return Ok(());
        }
        if !self.custom_index.has_app(&app_id) {
            self.loaded_custom_apps.insert(app_id);
            return Ok(());
        }

        let user_catalog =
            storage::load_app_user_catalog_from_customs_index(&self.custom_index, &app_id)?;
        self.catalog.merge_user_catalog(user_catalog);
        self.loaded_custom_apps.insert(app_id);
        Ok(())
    }

    fn show_cheatsheet(&mut self, ui: &mut egui::Ui, palette: UiPalette) {
        let active_app_id = normalize_app_id(&self.active.app_id);
        if let Err(error) = self.ensure_user_shortcuts_loaded(&active_app_id) {
            self.status = format!("사용자 정의 단축키를 읽지 못했습니다: {error:#}");
        }
        let sheet = self.catalog.sheet_for(&active_app_id);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(active_app_id)
                .size(26.0)
                .strong()
                .color(palette.heading),
        );
        ui.add_space(22.0);

        if sheet.shortcuts.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("등록된 단축키가 없습니다.")
                        .size(16.0)
                        .color(palette.weak_text),
                );
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            show_shortcut_columns(ui, &sheet.shortcuts, palette);
        });
    }
}

fn is_toggle_event(hotkey: Option<HotKey>, event: GlobalHotKeyEvent) -> bool {
    Some(event.id) == hotkey.map(|hotkey| hotkey.id()) && event.state == HotKeyState::Pressed
}

fn parse_hotkey_for_registration(combo: &str) -> Result<HotKey> {
    let parts = combo
        .split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "win" | "windows" | "meta" => "Super".to_owned(),
            "control" => "Ctrl".to_owned(),
            "esc" => "Escape".to_owned(),
            other => other.to_owned(),
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return Err(anyhow!("빈 단축키입니다."));
    }

    parts
        .join("+")
        .parse::<HotKey>()
        .with_context(|| format!("지원하지 않는 단축키입니다: {combo}"))
}

fn capture_combo_from_events(ctx: &egui::Context) -> Option<String> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            {
                combo_from_egui_key(*key, *modifiers)
            } else {
                None
            }
        })
    })
}

fn combo_from_egui_key(key: egui::Key, modifiers: egui::Modifiers) -> Option<String> {
    let key_name = egui_key_name(key)?;
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.mac_cmd {
        parts.push("Win");
    }
    parts.push(key_name);
    Some(parts.join("+"))
}

fn egui_key_name(key: egui::Key) -> Option<&'static str> {
    Some(match key {
        egui::Key::ArrowDown => "Down",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        egui::Key::ArrowUp => "Up",
        egui::Key::Escape => "Escape",
        egui::Key::Tab => "Tab",
        egui::Key::Backspace => "Backspace",
        egui::Key::Enter => "Enter",
        egui::Key::Space => "Space",
        egui::Key::Insert => "Insert",
        egui::Key::Delete => "Delete",
        egui::Key::Home => "Home",
        egui::Key::End => "End",
        egui::Key::PageUp => "PageUp",
        egui::Key::PageDown => "PageDown",
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        egui::Key::Backtick => "`",
        egui::Key::Minus => "-",
        egui::Key::Equals => "=",
        egui::Key::OpenBracket => "[",
        egui::Key::CloseBracket => "]",
        egui::Key::Backslash => "\\",
        egui::Key::Semicolon => ";",
        egui::Key::Quote => "'",
        egui::Key::Comma => ",",
        egui::Key::Period => ".",
        egui::Key::Slash => "/",
        _ => return None,
    })
}

fn shortcut_capture_button(
    ui: &mut egui::Ui,
    combo: &str,
    active: bool,
    palette: UiPalette,
) -> egui::Response {
    let text = if active {
        "누를 단축키 입력..."
    } else if combo.trim().is_empty() {
        "단축키 입력"
    } else {
        combo
    };
    let fill = if active {
        palette.button_active
    } else {
        palette.extreme_bg
    };
    ui.add_sized(
        [260.0, 28.0],
        egui::Button::new(egui::RichText::new(text).monospace())
            .fill(fill)
            .stroke(egui::Stroke::new(0.0, egui::Color32::TRANSPARENT)),
    )
}

fn borderless_text_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .frame(egui::Frame::NONE)
        .margin(egui::vec2(6.0, 4.0))
}

fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    palette: UiPalette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.label(
        egui::RichText::new(title)
            .size(15.0)
            .strong()
            .color(palette.heading),
    );
    ui.add_space(8.0);
    add_contents(ui);
}

fn show_shortcut_columns(ui: &mut egui::Ui, shortcuts: &[ShortcutEntry], palette: UiPalette) {
    let grouped = grouped_shortcuts(shortcuts);
    let available_width = ui.available_width();
    let column_count = if available_width >= 1120.0 {
        4
    } else if available_width >= 820.0 {
        3
    } else if available_width >= 560.0 {
        2
    } else {
        1
    };

    ui.columns(column_count, |columns| {
        for (group_index, (group, entries)) in grouped.iter().enumerate() {
            let column_index = group_index % column_count;
            let column = &mut columns[column_index];
            if group_index >= column_count {
                column.add_space(22.0);
            }
            column.label(
                egui::RichText::new(group)
                    .font(group_heading_font_id())
                    .strong()
                    .color(palette.group_heading),
            );
            column.add_space(8.0);
            for entry in entries {
                column.horizontal(|ui| {
                    ui.set_min_height(22.0);
                    ui.add_sized(
                        [118.0, 20.0],
                        egui::Label::new(
                            egui::RichText::new(&entry.combo)
                                .monospace()
                                .color(palette.text),
                        ),
                    );
                    ui.label(
                        egui::RichText::new(&entry.action)
                            .size(13.0)
                            .color(palette.text),
                    );
                });
            }
        }
    });
}

fn grouped_shortcuts(shortcuts: &[ShortcutEntry]) -> Vec<(String, Vec<&ShortcutEntry>)> {
    let mut grouped: BTreeMap<String, Vec<&ShortcutEntry>> = BTreeMap::new();
    for entry in shortcuts {
        grouped
            .entry(entry.group.trim().to_owned())
            .or_default()
            .push(entry);
    }
    grouped.into_iter().collect()
}

fn append_status(status: &mut String, message: String) {
    if status.is_empty() {
        *status = message;
    } else {
        status.push_str("; ");
        status.push_str(&message);
    }
}

fn build_tray_icon() -> Result<TrayIcon> {
    let settings_item = MenuItem::with_id(TRAY_MENU_SETTINGS, "설정", true, None);
    let close_item = MenuItem::with_id(TRAY_MENU_CLOSE, "닫기", true, None);
    let menu = Menu::with_items(&[&settings_item, &close_item])?;
    let icon = make_tray_icon()?;

    TrayIconBuilder::new()
        .with_tooltip(APP_NAME)
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_menu_on_left_click(true)
        .with_menu_on_right_click(true)
        .build()
        .context("failed to build tray icon")
}

fn make_tray_icon() -> Result<Icon> {
    let width = 32;
    let height = 32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let in_mark = (7..=24).contains(&x) && (9..=13).contains(&y)
                || (7..=12).contains(&x) && (9..=24).contains(&y)
                || (7..=24).contains(&x) && (20..=24).contains(&y);
            let (r, g, b, a) = if in_mark {
                (244, 248, 252, 255)
            } else {
                (45, 55, 68, 255)
            };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    Icon::from_rgba(rgba, width, height).context("failed to create tray icon")
}

fn install_korean_font(ctx: &egui::Context) -> Result<()> {
    let regular_font_bytes =
        fs::read(r"C:\Windows\Fonts\malgun.ttf").context("failed to read Malgun Gothic font")?;
    let bold_font_bytes = fs::read(r"C:\Windows\Fonts\malgunbd.ttf")
        .context("failed to read Malgun Gothic Bold font")?;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        KOREAN_FONT_REGULAR.to_owned(),
        Arc::new(egui::FontData::from_owned(regular_font_bytes)),
    );
    fonts.font_data.insert(
        KOREAN_FONT_BOLD.to_owned(),
        Arc::new(egui::FontData::from_owned(bold_font_bytes)),
    );
    fonts.families.insert(
        egui::FontFamily::Name(Arc::from(KOREAN_BOLD_FONT_FAMILY)),
        vec![KOREAN_FONT_BOLD.to_owned(), KOREAN_FONT_REGULAR.to_owned()],
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, KOREAN_FONT_REGULAR.to_owned());
    }
    ctx.set_fonts(fonts);
    Ok(())
}

fn group_heading_font_id() -> egui::FontId {
    egui::FontId::new(
        16.0,
        egui::FontFamily::Name(Arc::from(KOREAN_BOLD_FONT_FAMILY)),
    )
}

#[derive(Debug, Clone, Copy)]
struct UiPalette {
    background: egui::Color32,
    text: egui::Color32,
    weak_text: egui::Color32,
    faint_bg: egui::Color32,
    extreme_bg: egui::Color32,
    button_bg: egui::Color32,
    button_hover: egui::Color32,
    button_active: egui::Color32,
    heading: egui::Color32,
    group_heading: egui::Color32,
}

fn palette_for(theme: egui::Theme) -> UiPalette {
    match theme {
        egui::Theme::Dark => UiPalette {
            background: egui::Color32::from_rgb(24, 24, 24),
            text: egui::Color32::from_rgb(230, 232, 235),
            weak_text: egui::Color32::from_rgb(158, 164, 174),
            faint_bg: egui::Color32::from_rgb(42, 45, 50),
            extreme_bg: egui::Color32::from_rgb(13, 16, 20),
            button_bg: egui::Color32::from_rgb(48, 54, 62),
            button_hover: egui::Color32::from_rgb(61, 68, 78),
            button_active: egui::Color32::from_rgb(80, 91, 106),
            heading: egui::Color32::from_rgb(248, 249, 250),
            group_heading: egui::Color32::from_rgb(255, 255, 255),
        },
        egui::Theme::Light => UiPalette {
            background: egui::Color32::from_rgb(247, 248, 250),
            text: egui::Color32::from_rgb(29, 35, 43),
            weak_text: egui::Color32::from_rgb(86, 96, 110),
            faint_bg: egui::Color32::from_rgb(224, 230, 238),
            extreme_bg: egui::Color32::from_rgb(255, 255, 255),
            button_bg: egui::Color32::from_rgb(218, 225, 234),
            button_hover: egui::Color32::from_rgb(204, 215, 228),
            button_active: egui::Color32::from_rgb(184, 200, 220),
            heading: egui::Color32::from_rgb(11, 17, 25),
            group_heading: egui::Color32::from_rgb(0, 0, 0),
        },
    }
}

fn current_window_placement(ctx: &egui::Context) -> Option<WindowPlacement> {
    ctx.input(|input| {
        let viewport = input.viewport();
        let inner_rect = viewport.inner_rect?;
        let position = viewport
            .outer_rect
            .map(|rect| rect.min)
            .unwrap_or(inner_rect.min);
        let mut placement = WindowPlacement {
            x: position.x,
            y: position.y,
            width: inner_rect.width(),
            height: inner_rect.height(),
        };
        placement.normalize();
        Some(placement)
    })
}

fn resolved_theme(ctx: &egui::Context, mode: ThemeMode) -> egui::Theme {
    match mode {
        ThemeMode::Default => ctx.system_theme().unwrap_or(egui::Theme::Dark),
        ThemeMode::Light => egui::Theme::Light,
        ThemeMode::Dark => egui::Theme::Dark,
    }
}

fn with_opacity(color: egui::Color32, opacity: f32) -> egui::Color32 {
    let alpha = (opacity.clamp(0.55, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn configure_style(ctx: &egui::Context, mode: ThemeMode) {
    let theme = resolved_theme(ctx, mode);
    let palette = palette_for(theme);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = theme.default_visuals();
    apply_palette_to_visuals(&mut style.visuals, palette);
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_global_style(style);
}

fn apply_palette_to_visuals(visuals: &mut egui::Visuals, palette: UiPalette) {
    let no_stroke = egui::Stroke::new(0.0, egui::Color32::TRANSPARENT);
    visuals.override_text_color = Some(palette.text);
    visuals.weak_text_color = Some(palette.weak_text);
    visuals.panel_fill = palette.background;
    visuals.window_fill = palette.background;
    visuals.faint_bg_color = palette.faint_bg;
    visuals.extreme_bg_color = palette.extreme_bg;
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
    visuals.widgets.inactive.fg_stroke.color = palette.text;
    visuals.widgets.hovered.fg_stroke.color = palette.heading;
    visuals.widgets.active.fg_stroke.color = palette.heading;
    visuals.widgets.inactive.bg_fill = palette.button_bg;
    visuals.widgets.inactive.weak_bg_fill = palette.button_bg;
    visuals.widgets.hovered.bg_fill = palette.button_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.button_hover;
    visuals.widgets.active.bg_fill = palette.button_active;
    visuals.widgets.active.weak_bg_fill = palette.button_active;
    visuals.widgets.noninteractive.bg_stroke = no_stroke;
    visuals.widgets.inactive.bg_stroke = no_stroke;
    visuals.widgets.hovered.bg_stroke = no_stroke;
    visuals.widgets.active.bg_stroke = no_stroke;
    visuals.widgets.open.bg_stroke = no_stroke;
}

impl eframe::App for CheatSheetApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.remember_repaint_context(ctx);
        self.poll_hotkey(ctx);
        self.poll_tray(ctx);
        self.persist_window_settings_if_changed(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.remember_repaint_context(&ctx);
        if !self.visible {
            return;
        }

        if self.capture_target.is_none() && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        }

        configure_style(&ctx, self.settings.theme);
        let theme = resolved_theme(&ctx, self.settings.theme);
        let palette = palette_for(theme);
        {
            let visuals = ui.visuals_mut();
            apply_palette_to_visuals(visuals, palette);
        }

        let background = with_opacity(palette.background, self.settings.opacity);
        ui.painter().rect_filled(ui.max_rect(), 0.0, background);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(ui.max_rect().shrink(28.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_min_size(ui.available_size());
                match self.view {
                    AppView::CheatSheet => self.show_cheatsheet(ui, palette),
                    AppView::Settings => self.show_settings(ui, &ctx, palette),
                }
            },
        );
    }

    fn on_exit(&mut self) {
        let _ = storage::save_app_settings(&self.settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    #[test]
    fn toggle_event_matches_registered_hotkey_press_only() {
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
        let press = GlobalHotKeyEvent {
            id: hotkey.id(),
            state: HotKeyState::Pressed,
        };
        let release = GlobalHotKeyEvent {
            id: hotkey.id(),
            state: HotKeyState::Released,
        };

        assert!(is_toggle_event(Some(hotkey), press));
        assert!(!is_toggle_event(Some(hotkey), release));
    }

    #[test]
    fn parses_display_hotkey_for_registration() {
        let hotkey = parse_hotkey_for_registration("Ctrl+Shift+Space").unwrap();

        assert_eq!(hotkey.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(hotkey.key, Code::Space);
    }

    #[test]
    fn captured_key_combo_uses_display_format() {
        let combo = combo_from_egui_key(
            egui::Key::Space,
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(combo, "Ctrl+Shift+Space");
    }

    #[test]
    fn group_heading_palette_is_stronger_than_regular_heading() {
        let dark = palette_for(egui::Theme::Dark);
        let light = palette_for(egui::Theme::Light);

        assert!(dark.group_heading.r() >= dark.heading.r());
        assert!(light.group_heading.r() <= light.heading.r());
    }

    #[test]
    fn group_heading_uses_bold_korean_font_family() {
        let font_id = group_heading_font_id();

        assert_eq!(font_id.size, 16.0);
        assert_eq!(
            font_id.family,
            egui::FontFamily::Name(std::sync::Arc::from(KOREAN_BOLD_FONT_FAMILY))
        );
    }
}
