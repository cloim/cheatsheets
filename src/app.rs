use crate::core::{
    AppIdentity, AppSheetConfig, Catalog, ShortcutEntry, ShortcutSource, UserShortcutPatch,
    normalize_app_id,
};
use crate::import::{ImportFormat, parse_shortcut_import};
use crate::storage::{AppSettings, OverlayStyleSettings, RgbaColor, ThemeMode, WindowPlacement};
use crate::{platform, storage};
use anyhow::{Context, Result, anyhow};
use eframe::egui;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use std::{
    collections::BTreeMap,
    env, fs,
    ops::Range,
    process::Command,
    sync::{Arc, Mutex, mpsc},
};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

const APP_NAME: &str = "CheatSheets";
const TRAY_MENU_SETTINGS: &str = "cheatsheets.settings";
const TRAY_MENU_RESTART: &str = "cheatsheets.restart";
const TRAY_MENU_CLOSE: &str = "cheatsheets.close";
const KOREAN_FONT_REGULAR: &str = "malgun_gothic";
const KOREAN_FONT_BOLD: &str = "malgun_gothic_bold";
const KOREAN_BOLD_FONT_FAMILY: &str = "korean_bold";

pub struct CheatSheetsApp {
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
    active_app_sheet_config: AppSheetConfig,
    settings: AppSettings,
    view: AppView,
    settings_popup_open: bool,
    settings_popup_needs_focus: bool,
    settings_section: SettingsSection,
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
    Shortcuts,
}

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

    fn description(self) -> &'static str {
        match self {
            Self::General => "앱 화면과 전체 표시 방식을 설정합니다.",
            Self::OverlayDisplay => "오버레이 카드의 표시 스타일과 레이아웃을 설정합니다.",
            Self::Hotkeys => "오버레이를 여닫는 전역 단축키를 설정합니다.",
            Self::ShortcutEditor => "현재 앱의 사용자 단축키를 추가하거나 수정합니다.",
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

fn settings_status_text(status: &str) -> Option<&str> {
    (!status.is_empty()).then_some(status)
}

fn viewport_decorations_for_view(_view: AppView) -> bool {
    false
}

fn viewport_resizable_for_view(_view: AppView) -> bool {
    false
}

fn settings_popup_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("cheatsheets_settings_popup")
}

const SETTINGS_SIDEBAR_WIDTH: f32 = 208.0;
const SETTINGS_TWO_COLUMN_MIN_WIDTH: f32 = 720.0;
const SETTINGS_CARD_GAP: f32 = 8.0;

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

fn settings_popup_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("CheatSheets Settings")
        .with_inner_size([1080.0, 680.0])
        .with_min_inner_size([760.0, 560.0])
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
}

fn open_settings_popup_state(open: &mut bool, needs_focus: &mut bool) {
    *open = true;
    *needs_focus = true;
}

fn close_settings_popup_state(open: &mut bool, capture_target: &mut Option<CaptureTarget>) {
    *open = false;
    *capture_target = None;
}

fn settings_popup_should_close_on_escape(
    capture_target: Option<CaptureTarget>,
    escape_pressed: bool,
) -> bool {
    capture_target.is_none() && escape_pressed
}

fn should_os_hide_root_overlay(settings_popup_open: bool) -> bool {
    !settings_popup_open
}

fn transparent_clear_color() -> [f32; 4] {
    egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureTarget {
    ToggleHotkey,
    ShortcutCombo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMenuAction {
    Settings,
    Restart,
    Close,
}

impl CheatSheetsApp {
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
            active_app_sheet_config: AppSheetConfig::default(),
            settings,
            view: AppView::Shortcuts,
            settings_popup_open: false,
            settings_popup_needs_focus: false,
            settings_section: SettingsSection::default(),
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
                    let action = tray_action_for_menu_id(event.id.as_ref());
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
                    self.view = AppView::Shortcuts;
                    apply_viewport_chrome(ctx, self.view);
                    if self.settings_popup_open {
                        self.settings_popup_needs_focus = true;
                    }
                }
                self.visible = will_show;
                if will_show || should_os_hide_root_overlay(self.settings_popup_open) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.visible));
                } else {
                    self.settings_popup_needs_focus = true;
                }
                ctx.request_repaint();
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
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
                TrayMenuAction::Restart => match restart_application() {
                    Ok(()) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        append_status(&mut self.status, format!("재시작 실패: {error:#}"));
                        self.visible = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        ctx.request_repaint();
                    }
                },
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
            Ok(path) => match self.refresh_custom_index() {
                Ok(()) => self.status = format!("저장했습니다: {}", path.display()),
                Err(error) => {
                    self.status = format!(
                        "저장했지만 사용자 정의 파일 목록을 다시 읽지 못했습니다: {error:#}"
                    )
                }
            },
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
        if self
            .settings
            .window
            .is_none_or(|saved| window.differs_from(&saved))
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
            self.refresh_custom_index()?;
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

    fn show_settings_popup_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        palette: UiPalette,
    ) {
        let available = ui.available_size();
        ui.allocate_ui_with_layout(
            available,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(SETTINGS_SIDEBAR_WIDTH, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.heading("설정");
                        ui.add_space(12.0);
                        self.show_settings_sidebar(ui, palette);
                    },
                );
                ui.separator();

                let content_size = egui::vec2(ui.available_width(), available.y);
                ui.allocate_ui_with_layout(
                    content_size,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let reset_overlay = self.show_settings_header(ui, palette);
                        if reset_overlay {
                            self.settings.overlay_style = OverlayStyleSettings::default();
                            self.settings.overlay_style.normalize();
                            self.persist_settings();
                            ctx.request_repaint();
                        }
                        ui.add_space(8.0);
                        self.show_settings_status(ui, palette);
                        let scroll_height = ui.available_height();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .min_scrolled_height(scroll_height)
                            .max_height(scroll_height)
                            .show(ui, |ui| match self.settings_section {
                                SettingsSection::General => {
                                    ui.set_min_height(scroll_height);
                                    self.show_settings_general(ui, ctx, palette)
                                }
                                SettingsSection::OverlayDisplay => {
                                    ui.set_min_height(scroll_height);
                                    self.show_settings_overlay_display(ui, ctx, scroll_height)
                                }
                                SettingsSection::Hotkeys => {
                                    ui.set_min_height(scroll_height);
                                    self.show_settings_hotkeys(ui, palette)
                                }
                                SettingsSection::ShortcutEditor => {
                                    ui.set_min_height(scroll_height);
                                    self.show_settings_shortcut_editor(ui, palette)
                                }
                            });
                    },
                );
            },
        );
    }

    fn show_settings_header(&mut self, ui: &mut egui::Ui, palette: UiPalette) -> bool {
        let mut reset_overlay = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(
                    egui::RichText::new(self.settings_section.label())
                        .color(palette.heading)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(self.settings_section.description())
                        .color(palette.weak_text),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.settings_section == SettingsSection::OverlayDisplay
                    && ui.button("기본값으로 되돌리기").clicked()
                {
                    reset_overlay = true;
                }
            });
        });
        reset_overlay
    }

    fn show_settings_status(&self, ui: &mut egui::Ui, palette: UiPalette) {
        if let Some(status) = settings_status_text(&self.status) {
            ui.label(
                egui::RichText::new(status)
                    .size(12.0)
                    .color(palette.weak_text),
            );
            ui.add_space(8.0);
        }
    }

    fn show_settings_sidebar(&mut self, ui: &mut egui::Ui, palette: UiPalette) {
        for section in settings_sections() {
            let selected = self.settings_section == section;
            let text_color = if selected {
                palette.heading
            } else {
                palette.weak_text
            };
            let fill = if selected {
                palette.button_active
            } else {
                palette.background
            };
            let response = ui.add_sized(
                [SETTINGS_SIDEBAR_WIDTH, 36.0],
                egui::Button::new(egui::RichText::new(section.label()).color(text_color))
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, palette.button_bg))
                    .corner_radius(8.0)
                    .selected(selected),
            );
            if response.clicked() {
                self.settings_section = section;
            }
        }
    }

    fn show_settings_general(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        palette: UiPalette,
    ) {
        settings_section(ui, "일반", palette, |ui| {
            settings_card(ui, "화면", palette, |ui| {
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
                            egui::Slider::new(&mut self.settings.opacity, 0.55..=1.0)
                                .show_value(false),
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
        });
    }

    fn show_settings_overlay_display(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        content_min_height: f32,
    ) {
        let palette = palette_for(resolved_theme(ctx, self.settings.theme));
        settings_section(ui, "오버레이 표시", palette, |ui| {
            let mut changed = false;
            let use_columns = settings_card_column_count(ui.available_width()) == 2;
            let card_min_height = if use_columns {
                (content_min_height - 42.0).max(0.0)
            } else {
                0.0
            };
            if use_columns {
                ui.columns(2, |columns| {
                    changed |= self.show_overlay_text_and_layout_card(
                        &mut columns[0],
                        palette,
                        card_min_height,
                    );
                    changed |= self.show_overlay_color_and_options_card(
                        &mut columns[1],
                        palette,
                        card_min_height,
                    );
                });
            } else {
                changed |= self.show_overlay_text_and_layout_card(ui, palette, 0.0);
                ui.add_space(SETTINGS_CARD_GAP);
                changed |= self.show_overlay_color_and_options_card(ui, palette, 0.0);
            }

            if changed {
                self.settings.overlay_style.normalize();
                self.persist_settings();
                ctx.request_repaint();
            }
        });
    }

    fn show_overlay_text_and_layout_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: UiPalette,
        min_height: f32,
    ) -> bool {
        let mut changed = false;
        settings_card_with_min_height(ui, "텍스트 크기", palette, min_height, |ui| {
            changed |= overlay_style_number_row(
                ui,
                "제목 크기",
                &mut self.settings.overlay_style.title_size,
                10.0..=36.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "설명 크기",
                &mut self.settings.overlay_style.subtitle_size,
                8.0..=24.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "그룹 크기",
                &mut self.settings.overlay_style.group_heading_size,
                8.0..=24.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "동작 크기",
                &mut self.settings.overlay_style.action_text_size,
                8.0..=24.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "키캡 글자",
                &mut self.settings.overlay_style.keycap_text_size,
                7.0..=18.0,
                "px",
            );

            ui.separator();
            ui.label(
                egui::RichText::new("레이아웃")
                    .strong()
                    .color(palette.heading),
            );
            ui.add_space(4.0);

            changed |= overlay_style_number_row(
                ui,
                "안쪽 여백",
                &mut self.settings.overlay_style.card_padding,
                0.0..=64.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "행 높이",
                &mut self.settings.overlay_style.row_height,
                12.0..=40.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "키캡 높이",
                &mut self.settings.overlay_style.keycap_height,
                10.0..=28.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "키 영역 너비",
                &mut self.settings.overlay_style.combo_width,
                64.0..=220.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "동작 간격",
                &mut self.settings.overlay_style.action_gap,
                0.0..=32.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "동작 세로 보정",
                &mut self.settings.overlay_style.action_text_y_offset,
                -6.0..=6.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "키캡 간격",
                &mut self.settings.overlay_style.keycap_gap,
                0.0..=16.0,
                "px",
            );
            changed |= overlay_style_number_row(
                ui,
                "모서리",
                &mut self.settings.overlay_style.card_radius,
                0.0..=24.0,
                "px",
            );
        });

        changed
    }

    fn show_overlay_color_and_options_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: UiPalette,
        min_height: f32,
    ) -> bool {
        let mut changed = false;
        settings_card_with_min_height(ui, "색상", palette, min_height, |ui| {
            changed |=
                rgba_color_edit(ui, "카드", &mut self.settings.overlay_style.card_background);
            changed |= rgba_color_edit(
                ui,
                "카드 테두리",
                &mut self.settings.overlay_style.card_border,
            );
            changed |= rgba_color_edit(ui, "제목", &mut self.settings.overlay_style.title_color);
            changed |= rgba_color_edit(
                ui,
                "그룹 제목",
                &mut self.settings.overlay_style.group_heading_color,
            );
            changed |= rgba_color_edit(
                ui,
                "본문",
                &mut self.settings.overlay_style.action_text_color,
            );
            changed |=
                rgba_color_edit(ui, "보조", &mut self.settings.overlay_style.weak_text_color);
            changed |=
                rgba_color_edit(ui, "구분선", &mut self.settings.overlay_style.divider_color);
            changed |= rgba_color_edit(
                ui,
                "키캡 배경",
                &mut self.settings.overlay_style.keycap_background,
            );
            changed |= rgba_color_edit(
                ui,
                "키캡 테두리",
                &mut self.settings.overlay_style.keycap_border,
            );
            changed |= rgba_color_edit(
                ui,
                "키캡 글자",
                &mut self.settings.overlay_style.keycap_text_color,
            );

            ui.separator();
            ui.label(
                egui::RichText::new("표시 옵션")
                    .strong()
                    .color(palette.heading),
            );
            ui.add_space(4.0);

            changed |= settings_toggle_row(
                ui,
                "구분선 표시",
                &mut self.settings.overlay_style.show_column_dividers,
            );
            changed |= settings_toggle_row(
                ui,
                "리사이즈 그립 표시",
                &mut self.settings.overlay_style.show_resize_grip,
            );
            changed |= settings_toggle_row(
                ui,
                "빈 메시지 표시",
                &mut self.settings.overlay_style.show_empty_message,
            );
        });

        changed
    }

    fn show_settings_hotkeys(&mut self, ui: &mut egui::Ui, palette: UiPalette) {
        settings_section(ui, "단축키", palette, |ui| {
            settings_card(ui, "오버레이 단축키", palette, |ui| {
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
                if self.capture_target == Some(CaptureTarget::ToggleHotkey) {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("새 단축키를 누르세요.")
                            .size(12.0)
                            .color(palette.weak_text),
                    );
                }
            });
        });
    }

    fn show_settings_shortcut_editor(&mut self, ui: &mut egui::Ui, palette: UiPalette) {
        settings_section(ui, "단축키 편집", palette, |ui| {
            settings_card(ui, "단축키 추가 / 수정", palette, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "대상 프로세스: {}",
                        normalize_app_id(&self.active.app_id)
                    ))
                    .color(palette.weak_text),
                );
                ui.add_space(6.0);

                ui.label(
                    egui::RichText::new("단축키 정보")
                        .strong()
                        .color(palette.heading),
                );
                ui.add_space(4.0);
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
                            ui.add(
                                borderless_text_edit(&mut self.draft_group).desired_width(220.0),
                            );
                            if ui.button("저장").clicked() {
                                self.add_or_replace_user_shortcut();
                            }
                        });
                        ui.end_row();
                    });
                if self.capture_target == Some(CaptureTarget::ShortcutCombo) {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("저장할 단축키를 누르세요.")
                            .size(12.0)
                            .color(palette.weak_text),
                    );
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("가져오기")
                        .strong()
                        .color(palette.heading),
                );
                ui.add_space(4.0);
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
        });
    }

    fn refresh_custom_index(&mut self) -> Result<()> {
        self.custom_index = storage::load_user_catalog_index()?;
        Ok(())
    }

    fn ensure_user_shortcuts_loaded(&mut self, app_id: &str) -> Result<()> {
        let app_id = normalize_app_id(app_id);
        let is_active_app = app_id == normalize_app_id(&self.active.app_id);
        if !self.custom_index.has_app(&app_id) {
            self.catalog.replace_user_patches(app_id, Vec::new());
            if is_active_app {
                self.active_app_sheet_config = AppSheetConfig::default();
            }
            return Ok(());
        }

        let sheet = storage::load_app_sheet_from_customs_index(&self.custom_index, &app_id)?;
        let patches = sheet.patches;
        if is_active_app {
            self.active_app_sheet_config = sheet.config;
        }
        self.catalog.replace_user_patches(&app_id, patches);
        Ok(())
    }

    fn load_user_shortcuts_for_overlay(&mut self) {
        let active_app_id = normalize_app_id(&self.active.app_id);
        match self
            .refresh_custom_index()
            .and_then(|()| self.ensure_user_shortcuts_loaded(&active_app_id))
        {
            Ok(()) => {}
            Err(error) => {
                self.status = format!("사용자 정의 단축키를 읽지 못했습니다: {error:#}");
            }
        }
    }

    fn show_settings_popup(&mut self, ctx: &egui::Context, palette: UiPalette) {
        if !self.settings_popup_open {
            return;
        }
        let viewport_id = settings_popup_viewport_id();
        let viewport = settings_popup_viewport();
        ctx.show_viewport_immediate(viewport_id, viewport, |ui, _class| {
            let popup_ctx = ui.ctx().clone();
            if popup_ctx.input(|input| input.viewport().close_requested()) {
                close_settings_popup_state(&mut self.settings_popup_open, &mut self.capture_target);
                return;
            }
            if settings_popup_should_close_on_escape(
                self.capture_target,
                popup_ctx.input(|input| input.key_pressed(egui::Key::Escape)),
            ) {
                close_settings_popup_state(&mut self.settings_popup_open, &mut self.capture_target);
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

    fn show_shortcuts(&mut self, ui: &mut egui::Ui) {
        let active_app_id = normalize_app_id(&self.active.app_id);
        self.load_user_shortcuts_for_overlay();
        let sheet = self
            .catalog
            .sheet_for_with_config(&active_app_id, &self.active_app_sheet_config);
        let style = resolved_overlay_style(&self.settings.overlay_style, self.settings.opacity);
        let palette = style.palette;
        let card_rect = ui.max_rect().shrink(shortcut_card_outer_inset(self.view));
        ui.painter()
            .rect_filled(card_rect, style.card_radius, palette.fill);
        ui.painter().rect_stroke(
            card_rect,
            style.card_radius,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        let grip_rect = resize_grip_rect(card_rect);
        let resize_response = if should_show_resize_grip(style) {
            Some(show_resize_grip(ui, grip_rect, palette))
        } else {
            None
        };
        if let Some(resize_response) = resize_response.as_ref()
            && resize_response.dragged()
        {
            let current_size = current_viewport_size(ui.ctx()).unwrap_or(card_rect.size());
            let resized = resized_overlay_size(current_size, resize_response.drag_delta());
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(resized));
        }
        let drag_response = ui.interact(
            card_rect,
            ui.id().with("shortcut_card_drag"),
            shortcut_card_drag_sense(),
        );
        let pointer_in_resize_grip = should_show_resize_grip(style)
            && ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|pos| grip_rect.contains(pos));
        let resize_drag_started = resize_response
            .as_ref()
            .is_some_and(|response| response.drag_started());
        let resize_dragged = resize_response
            .as_ref()
            .is_some_and(|response| response.dragged());
        if drag_response.drag_started()
            && !pointer_in_resize_grip
            && !resize_drag_started
            && !resize_dragged
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let content_rect = card_rect.shrink(style.card_padding);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(&sheet.display_name)
                        .size(style.title_size)
                        .strong()
                        .color(palette.heading),
                );
                if let Some(description) = sheet.description.as_deref() {
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(description)
                            .size(style.subtitle_size)
                            .color(palette.weak_text),
                    );
                }
                ui.add_space(18.0);

                if sheet.shortcuts.is_empty() {
                    if should_show_empty_message(style) {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new("등록된 단축키가 없습니다.")
                                    .size(14.0)
                                    .color(palette.weak_text),
                            );
                        });
                    }
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    show_shortcut_columns(ui, &sheet.shortcuts, style);
                });
            },
        );
    }
}

const KEYCAP_TEXT_Y_OFFSET: f32 = -0.75;
const RESIZE_GRIP_SIZE: f32 = 18.0;
const RESIZE_GRIP_INSET: f32 = 6.0;
const GROUP_HEADING_GAP: f32 = 6.0;
const INTER_GROUP_GAP: f32 = 16.0;

#[allow(dead_code)]
fn target_action_width(measured_widths: &[f32]) -> f32 {
    assert!(
        measured_widths.iter().all(|width| width.is_finite()),
        "action width measurements must be finite"
    );
    if measured_widths.is_empty() {
        return 180.0;
    }

    let mut sorted_widths = measured_widths.to_vec();
    sorted_widths.sort_by(f32::total_cmp);
    let percentile_index = (sorted_widths.len() * 9).div_ceil(10) - 1;
    sorted_widths[percentile_index].clamp(180.0, 320.0)
}

#[allow(dead_code)]
fn target_combo_width(measured_widths: &[f32], configured_width: f32) -> f32 {
    assert!(
        configured_width.is_finite(),
        "configured combo width must be finite"
    );
    assert!(
        measured_widths.iter().all(|width| width.is_finite()),
        "combo width measurements must be finite"
    );
    if measured_widths.is_empty() {
        return configured_width.min(220.0);
    }

    let mut sorted_widths = measured_widths.to_vec();
    sorted_widths.sort_by(f32::total_cmp);
    let percentile_index = (sorted_widths.len() * 9).div_ceil(10) - 1;
    sorted_widths[percentile_index]
        .max(configured_width)
        .min(220.0)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct OverlayLayoutMetrics {
    target_combo_width: f32,
    target_action_width: f32,
    column_count: usize,
    column_width: f32,
    action_width: f32,
}

#[allow(dead_code)]
fn calculate_overlay_layout(
    available_content_width: f32,
    group_count: usize,
    target_combo_width: f32,
    target_action_width: f32,
    row_item_spacing: f32,
    action_gap: f32,
    column_gap: f32,
) -> OverlayLayoutMetrics {
    assert!(group_count > 0, "group count must be greater than zero");
    assert!(
        [
            available_content_width,
            target_combo_width,
            target_action_width,
            row_item_spacing,
            action_gap,
            column_gap,
        ]
        .iter()
        .all(|value| value.is_finite()),
        "overlay layout inputs must be finite"
    );

    for column_count in (1..=group_count.min(4)).rev() {
        let total_column_gap = column_count.saturating_sub(1) as f32 * column_gap;
        let column_width = (available_content_width - total_column_gap) / column_count as f32;
        let action_width = column_width - target_combo_width - row_item_spacing - action_gap;

        if action_width >= target_action_width {
            return OverlayLayoutMetrics {
                target_combo_width,
                target_action_width,
                column_count,
                column_width,
                action_width,
            };
        }
    }

    let column_width = available_content_width.max(0.0);
    OverlayLayoutMetrics {
        target_combo_width,
        target_action_width,
        column_count: 1,
        column_width,
        action_width: (column_width - target_combo_width - row_item_spacing - action_gap).max(0.0),
    }
}

fn group_range_height(
    group_block_heights: &[f32],
    inter_group_gap: f32,
    range: Range<usize>,
) -> f32 {
    assert!(!range.is_empty(), "group range must not be empty");
    assert!(
        range.end <= group_block_heights.len(),
        "group range must stay within the block heights"
    );
    group_block_heights[range.clone()].iter().sum::<f32>()
        + range.len().saturating_sub(1) as f32 * inter_group_gap
}

fn partition_group_ranges(
    group_block_heights: &[f32],
    inter_group_gap: f32,
    column_count: usize,
) -> Vec<Range<usize>> {
    assert!(
        !group_block_heights.is_empty(),
        "group block heights must not be empty"
    );
    assert!(
        group_block_heights
            .iter()
            .all(|height| height.is_finite() && *height >= 0.0),
        "group block heights must be finite and non-negative"
    );
    assert!(
        inter_group_gap.is_finite() && inter_group_gap >= 0.0,
        "inter-group gap must be finite and non-negative"
    );
    assert!(
        (1..=group_block_heights.len()).contains(&column_count),
        "column count must be between one and the group count"
    );

    let group_count = group_block_heights.len();
    let mut prefix_heights = Vec::with_capacity(group_count + 1);
    prefix_heights.push(0.0);
    for height in group_block_heights {
        prefix_heights.push(prefix_heights.last().copied().unwrap_or_default() + height);
    }
    let range_height = |start: usize, end: usize| {
        prefix_heights[end] - prefix_heights[start]
            + (end - start).saturating_sub(1) as f32 * inter_group_gap
    };

    let mut minimum_tallest = vec![vec![f32::INFINITY; group_count + 1]; column_count + 1];
    for (start, tallest) in minimum_tallest[1].iter_mut().take(group_count).enumerate() {
        *tallest = range_height(start, group_count);
    }
    for columns in 2..=column_count {
        for start in (0..=group_count - columns).rev() {
            let last_end = group_count - (columns - 1);
            for end in start + 1..=last_end {
                let candidate = range_height(start, end).max(minimum_tallest[columns - 1][end]);
                if candidate
                    .total_cmp(&minimum_tallest[columns][start])
                    .is_lt()
                {
                    minimum_tallest[columns][start] = candidate;
                }
            }
        }
    }

    let mut ranges = Vec::with_capacity(column_count);
    let mut start = 0;
    for columns in (2..=column_count).rev() {
        let target = minimum_tallest[columns][start];
        let last_end = group_count - (columns - 1);
        let end = (start + 1..=last_end)
            .find(|end| {
                range_height(start, *end)
                    .max(minimum_tallest[columns - 1][*end])
                    .total_cmp(&target)
                    .is_eq()
            })
            .expect("an optimal ordered partition boundary must exist");
        ranges.push(start..end);
        start = end;
    }
    ranges.push(start..group_count);
    ranges
}

#[derive(Debug, Clone, Copy)]
struct ShortcutCardPalette {
    fill: egui::Color32,
    border: egui::Color32,
    heading: egui::Color32,
    group_heading: egui::Color32,
    text: egui::Color32,
    weak_text: egui::Color32,
    divider: egui::Color32,
    keycap_fill: egui::Color32,
    keycap_border: egui::Color32,
    keycap_text: egui::Color32,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedOverlayStyle {
    palette: ShortcutCardPalette,
    title_size: f32,
    subtitle_size: f32,
    group_heading_size: f32,
    action_text_size: f32,
    action_text_y_offset: f32,
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
        action_text_y_offset: settings.action_text_y_offset,
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

fn rgba_to_color(color: RgbaColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn rgba_to_color_scaled_alpha(color: RgbaColor, opacity: f32) -> egui::Color32 {
    let alpha = (opacity.clamp(0.55, 1.0) * color.a as f32).round() as u8;
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, alpha)
}

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

fn view_content_inset(view: AppView) -> f32 {
    match view {
        AppView::Shortcuts => 0.0,
    }
}

fn shortcut_card_outer_inset(view: AppView) -> f32 {
    match view {
        AppView::Shortcuts => 0.0,
    }
}

fn measure_keycap_label_width(ui: &egui::Ui, label: &str, style: ResolvedOverlayStyle) -> f32 {
    (ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::FontId::monospace(style.keycap_text_size),
            style.palette.keycap_text,
        )
        .size()
        .x
        + 9.0)
        .max(16.0)
}

fn keycap_text_position(rect: egui::Rect) -> egui::Pos2 {
    rect.center() + egui::vec2(0.0, KEYCAP_TEXT_Y_OFFSET)
}

#[cfg(test)]
fn shortcut_action_text_position(rect: egui::Rect, y_offset: f32) -> egui::Pos2 {
    egui::pos2(rect.left(), rect.center().y + y_offset)
}

#[allow(dead_code)]
fn measure_combo_run_width(ui: &egui::Ui, parts: &[&str], style: ResolvedOverlayStyle) -> f32 {
    let labels_width = parts
        .iter()
        .map(|part| measure_keycap_label_width(ui, part, style))
        .sum::<f32>();
    let gaps = parts.len().saturating_sub(1) as f32 * style.keycap_gap;
    labels_width + gaps
}

#[derive(Debug, Clone)]
struct PreparedKeycap {
    label: String,
    width: f32,
}

#[derive(Debug, Clone)]
struct PreparedComboRun {
    keycaps: Vec<PreparedKeycap>,
    measured_width: f32,
}

fn prepare_combo_run(ui: &egui::Ui, combo: &str, style: ResolvedOverlayStyle) -> PreparedComboRun {
    let keycaps = combo_keycap_parts(combo)
        .into_iter()
        .map(|label| PreparedKeycap {
            label: label.to_owned(),
            width: measure_keycap_label_width(ui, label, style),
        })
        .collect::<Vec<_>>();
    let measured_width = keycaps.iter().map(|keycap| keycap.width).sum::<f32>()
        + keycaps.len().saturating_sub(1) as f32 * style.keycap_gap;
    PreparedComboRun {
        keycaps,
        measured_width,
    }
}

#[allow(dead_code)]
fn measure_action_width(ui: &egui::Ui, action: &str, style: ResolvedOverlayStyle) -> f32 {
    ui.painter()
        .layout_no_wrap(
            action.to_owned(),
            egui::FontId::proportional(style.action_text_size),
            style.palette.text,
        )
        .size()
        .x
}

#[derive(Debug, Clone)]
struct ShortcutActionLayout {
    galley: Arc<egui::Galley>,
    resolved_row_height: f32,
    galley_rect: egui::Rect,
    elided: bool,
}

#[allow(clippy::too_many_arguments)]
fn layout_shortcut_action(
    ui: &egui::Ui,
    action: &str,
    action_width: f32,
    max_rows: usize,
    action_font: egui::FontId,
    action_color: egui::Color32,
    configured_row_height: f32,
    keycap_height: f32,
    action_text_y_offset: f32,
) -> ShortcutActionLayout {
    let mut job =
        egui::text::LayoutJob::simple(action.to_owned(), action_font, action_color, action_width);
    job.wrap = egui::text::TextWrapping {
        max_width: action_width,
        max_rows,
        break_anywhere: true,
        ..Default::default()
    };
    let galley = ui.painter().layout_job(job);
    let resolved_row_height = configured_row_height
        .max(keycap_height)
        .max(galley.rect.height() + 2.0 * action_text_y_offset.abs());
    let galley_rect = egui::Rect::from_min_size(
        egui::pos2(
            0.0,
            (resolved_row_height - galley.rect.height()) / 2.0 + action_text_y_offset,
        ),
        galley.size(),
    );
    let elided = galley.elided;

    ShortcutActionLayout {
        galley,
        resolved_row_height,
        galley_rect,
        elided,
    }
}

fn overflow_tooltip_text(full_text: &str, overflowed: bool) -> Option<&str> {
    overflowed.then_some(full_text)
}

fn should_show_resize_grip(style: ResolvedOverlayStyle) -> bool {
    style.show_resize_grip
}

fn should_show_column_dividers(style: ResolvedOverlayStyle) -> bool {
    style.show_column_dividers
}

fn should_show_empty_message(style: ResolvedOverlayStyle) -> bool {
    style.show_empty_message
}

fn shortcut_card_drag_sense() -> egui::Sense {
    egui::Sense::drag()
}

fn resize_grip_rect(card_rect: egui::Rect) -> egui::Rect {
    let max = card_rect.max - egui::vec2(RESIZE_GRIP_INSET, RESIZE_GRIP_INSET);
    egui::Rect::from_min_max(max - egui::vec2(RESIZE_GRIP_SIZE, RESIZE_GRIP_SIZE), max)
}

fn resized_overlay_size(current_size: egui::Vec2, drag_delta: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (current_size.x + drag_delta.x).max(WindowPlacement::MIN_WIDTH),
        (current_size.y + drag_delta.y).max(WindowPlacement::MIN_HEIGHT),
    )
}

fn current_viewport_size(ctx: &egui::Context) -> Option<egui::Vec2> {
    ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()))
}

fn show_resize_grip(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    palette: ShortcutCardPalette,
) -> egui::Response {
    let response = ui
        .interact(
            rect,
            ui.id().with("shortcut_resize_grip"),
            egui::Sense::drag(),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeNwSe);

    let stroke = egui::Stroke::new(1.0, palette.weak_text.gamma_multiply(0.55));
    for offset in [4.0, 8.0, 12.0] {
        ui.painter().line_segment(
            [
                egui::pos2(rect.right() - offset, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 2.0, rect.bottom() - offset),
            ],
            stroke,
        );
    }
    response
}

fn keycap_combo_start_x(rect_left: f32, rect_right: f32, measured_total_run_width: f32) -> f32 {
    let start_x = rect_right - measured_total_run_width;
    if measured_total_run_width > rect_right - rect_left {
        start_x
    } else {
        start_x.max(rect_left)
    }
}

#[allow(dead_code)]
fn show_keycap_combo(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    combo: &str,
    style: ResolvedOverlayStyle,
) -> bool {
    let prepared = prepare_combo_run(ui, combo, style);
    paint_prepared_keycap_combo(ui, rect, &prepared, style)
}

fn paint_prepared_keycap_combo(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    prepared: &PreparedComboRun,
    style: ResolvedOverlayStyle,
) -> bool {
    let palette = style.palette;
    if prepared.keycaps.is_empty() {
        return false;
    }

    let measured_total_run_width = prepared.measured_width;
    let overflowed = measured_total_run_width > rect.width();
    let mut x = keycap_combo_start_x(rect.left(), rect.right(), measured_total_run_width);
    let y = rect.center().y - style.keycap_height / 2.0;
    let painter = ui.painter().with_clip_rect(rect);
    for keycap in &prepared.keycaps {
        let key_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(keycap.width, style.keycap_height),
        );
        painter.rect_filled(key_rect, 3.0, palette.keycap_fill);
        painter.rect_stroke(
            key_rect,
            3.0,
            egui::Stroke::new(0.8, palette.keycap_border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            keycap_text_position(key_rect),
            egui::Align2::CENTER_CENTER,
            &keycap.label,
            egui::FontId::monospace(style.keycap_text_size),
            palette.keycap_text,
        );
        x += keycap.width + style.keycap_gap;
    }
    overflowed
}

fn is_toggle_event(hotkey: Option<HotKey>, event: GlobalHotKeyEvent) -> bool {
    Some(event.id) == hotkey.map(|hotkey| hotkey.id()) && event.state == HotKeyState::Pressed
}

fn apply_viewport_chrome(ctx: &egui::Context, view: AppView) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
        viewport_decorations_for_view(view),
    ));
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(
        viewport_resizable_for_view(view),
    ));
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

fn rgba_color_edit(ui: &mut egui::Ui, label: &str, color: &mut RgbaColor) -> bool {
    let mut egui_color = rgba_to_color(*color);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = ui.color_edit_button_srgba(&mut egui_color).changed();
        });
    });
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

fn overlay_style_number_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = ui
                .add(
                    egui::DragValue::new(value)
                        .range(range)
                        .suffix(suffix)
                        .speed(0.1),
                )
                .changed();
        });
    });
    changed
}

fn settings_toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = ui.checkbox(value, "").changed();
        });
    });
    changed
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

fn settings_card(
    ui: &mut egui::Ui,
    title: &str,
    palette: UiPalette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let _ = settings_card_with_min_height(ui, title, palette, 0.0, add_contents);
}

fn settings_card_with_min_height(
    ui: &mut egui::Ui,
    title: &str,
    palette: UiPalette,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    egui::Frame::new()
        .fill(palette.faint_bg)
        .stroke(egui::Stroke::new(1.0, palette.button_bg))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            if min_height > 0.0 {
                ui.set_min_height(min_height);
            }
            ui.label(
                egui::RichText::new(title)
                    .size(15.0)
                    .strong()
                    .color(palette.heading),
            );
            ui.add_space(8.0);
            add_contents(ui);
        })
        .response
}

#[derive(Debug, Clone)]
struct PreparedShortcutRow<'a> {
    entry: &'a ShortcutEntry,
    combo_run: PreparedComboRun,
    action_layout: ShortcutActionLayout,
}

#[derive(Debug, Clone)]
struct PreparedShortcutGroup<'a> {
    #[cfg(test)]
    name: String,
    heading_galley: Arc<egui::Galley>,
    rows: Vec<PreparedShortcutRow<'a>>,
}

#[derive(Debug, Clone)]
struct PreparedShortcutOverlay<'a> {
    layout: OverlayLayoutMetrics,
    group_block_heights: Vec<f32>,
    group_ranges: Vec<Range<usize>>,
    groups: Vec<PreparedShortcutGroup<'a>>,
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ShortcutRowRenderReport {
    group_index: usize,
    column_index: usize,
    combo: String,
    action: String,
    row_rect: egui::Rect,
    combo_rect: egui::Rect,
    action_rect: egui::Rect,
    combo_run_width: f32,
    resolved_row_height: f32,
    combo_overflowed: bool,
    action_overflowed: bool,
    action_galley_rows: usize,
    action_max_rows: usize,
    combo_response_id: egui::Id,
    action_response_id: egui::Id,
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ShortcutRenderReport {
    layout: OverlayLayoutMetrics,
    heading_gap: f32,
    inter_group_gap: f32,
    row_item_spacing: f32,
    column_gap: f32,
    group_names: Vec<String>,
    group_heading_heights: Vec<f32>,
    group_row_heights: Vec<Vec<f32>>,
    group_block_heights: Vec<f32>,
    group_ranges: Vec<Range<usize>>,
    column_rects: Vec<egui::Rect>,
    rows: Vec<ShortcutRowRenderReport>,
}

fn prepare_shortcut_overlay<'a>(
    ui: &egui::Ui,
    shortcuts: &'a [ShortcutEntry],
    style: ResolvedOverlayStyle,
    available_width: f32,
    row_item_spacing: f32,
    column_gap: f32,
) -> PreparedShortcutOverlay<'a> {
    let grouped = grouped_shortcuts(shortcuts);
    assert!(!grouped.is_empty(), "shortcut groups must not be empty");

    let combo_runs = grouped
        .iter()
        .map(|(_, entries)| {
            entries
                .iter()
                .map(|entry| prepare_combo_run(ui, &entry.combo, style))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let measured_combo_widths = combo_runs
        .iter()
        .flatten()
        .map(|run| run.measured_width)
        .collect::<Vec<_>>();
    let measured_action_widths = grouped
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .map(|entry| measure_action_width(ui, &entry.action, style))
        .collect::<Vec<_>>();
    let layout = calculate_overlay_layout(
        available_width,
        grouped.len(),
        target_combo_width(&measured_combo_widths, style.combo_width),
        target_action_width(&measured_action_widths),
        row_item_spacing,
        style.action_gap,
        column_gap,
    );
    let action_max_rows = if layout.column_count == 1 { 2 } else { 1 };
    let groups = grouped
        .into_iter()
        .zip(combo_runs)
        .map(|((name, entries), combo_runs)| {
            #[cfg(test)]
            let heading_text = name.clone();
            #[cfg(not(test))]
            let heading_text = name;
            let heading_galley = ui.painter().layout_no_wrap(
                heading_text,
                group_heading_font_id(style.group_heading_size),
                style.palette.group_heading,
            );
            let rows = entries
                .into_iter()
                .zip(combo_runs)
                .map(|(entry, combo_run)| PreparedShortcutRow {
                    entry,
                    combo_run,
                    action_layout: layout_shortcut_action(
                        ui,
                        &entry.action,
                        layout.action_width,
                        action_max_rows,
                        egui::FontId::proportional(style.action_text_size),
                        style.palette.text,
                        style.row_height,
                        style.keycap_height,
                        style.action_text_y_offset,
                    ),
                })
                .collect::<Vec<_>>();
            PreparedShortcutGroup {
                #[cfg(test)]
                name,
                heading_galley,
                rows,
            }
        })
        .collect::<Vec<_>>();
    let group_block_heights = groups
        .iter()
        .map(|group| {
            group.heading_galley.rect.height()
                + GROUP_HEADING_GAP
                + group
                    .rows
                    .iter()
                    .map(|row| row.action_layout.resolved_row_height)
                    .sum::<f32>()
        })
        .collect::<Vec<_>>();
    let group_ranges =
        partition_group_ranges(&group_block_heights, INTER_GROUP_GAP, layout.column_count);

    PreparedShortcutOverlay {
        layout,
        group_block_heights,
        group_ranges,
        groups,
    }
}

fn show_shortcut_columns(
    ui: &mut egui::Ui,
    shortcuts: &[ShortcutEntry],
    style: ResolvedOverlayStyle,
) {
    #[cfg(test)]
    {
        let mut report = None;
        render_shortcut_columns(ui, shortcuts, style, &mut report);
    }
    #[cfg(not(test))]
    render_shortcut_columns(ui, shortcuts, style);
}

fn render_shortcut_columns(
    ui: &mut egui::Ui,
    shortcuts: &[ShortcutEntry],
    style: ResolvedOverlayStyle,
    #[cfg(test)] report_slot: &mut Option<ShortcutRenderReport>,
) {
    let palette = style.palette;
    let available_width = ui.available_width().max(0.0);
    let row_item_spacing = ui.spacing().item_spacing.x;
    let column_gap = ui.spacing().item_spacing.x;
    let prepared = prepare_shortcut_overlay(
        ui,
        shortcuts,
        style,
        available_width,
        row_item_spacing,
        column_gap,
    );
    let tallest_column = prepared
        .group_ranges
        .iter()
        .map(|range| {
            group_range_height(
                &prepared.group_block_heights,
                INTER_GROUP_GAP,
                range.clone(),
            )
        })
        .max_by(f32::total_cmp)
        .expect("a prepared overlay must have at least one column");
    let (overlay_rect, _) = ui.allocate_exact_size(
        egui::vec2(available_width, tallest_column),
        egui::Sense::hover(),
    );
    let column_rects = prepared
        .group_ranges
        .iter()
        .enumerate()
        .map(|(column_index, range)| {
            let height = group_range_height(
                &prepared.group_block_heights,
                INTER_GROUP_GAP,
                range.clone(),
            );
            egui::Rect::from_min_size(
                egui::pos2(
                    overlay_rect.left()
                        + column_index as f32 * (prepared.layout.column_width + column_gap),
                    overlay_rect.top(),
                ),
                egui::vec2(prepared.layout.column_width, height),
            )
        })
        .collect::<Vec<_>>();

    if should_show_column_dividers(style) {
        let painter = ui.painter().with_clip_rect(overlay_rect);
        for column_rect in column_rects
            .iter()
            .take(column_rects.len().saturating_sub(1))
        {
            let x = column_rect.right() + column_gap / 2.0;
            painter.line_segment(
                [
                    egui::pos2(x, overlay_rect.top() + 4.0),
                    egui::pos2(x, overlay_rect.bottom() - 4.0),
                ],
                egui::Stroke::new(1.0, palette.divider),
            );
        }
    }

    #[cfg(test)]
    let mut report = ShortcutRenderReport {
        layout: prepared.layout,
        heading_gap: GROUP_HEADING_GAP,
        inter_group_gap: INTER_GROUP_GAP,
        row_item_spacing,
        column_gap,
        group_names: prepared
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect(),
        group_heading_heights: prepared
            .groups
            .iter()
            .map(|group| group.heading_galley.rect.height())
            .collect(),
        group_row_heights: prepared
            .groups
            .iter()
            .map(|group| {
                group
                    .rows
                    .iter()
                    .map(|row| row.action_layout.resolved_row_height)
                    .collect()
            })
            .collect(),
        group_block_heights: prepared.group_block_heights.clone(),
        group_ranges: prepared.group_ranges.clone(),
        column_rects: column_rects.clone(),
        rows: Vec::with_capacity(shortcuts.len()),
    };

    for (column_index, range) in prepared.group_ranges.iter().enumerate() {
        let column_rect = column_rects[column_index];
        let column_painter = ui.painter().with_clip_rect(column_rect);
        let mut y = column_rect.top();
        for group_index in range.clone() {
            if group_index > range.start {
                y += INTER_GROUP_GAP;
            }
            let group = &prepared.groups[group_index];
            column_painter.galley(
                egui::pos2(column_rect.left(), y),
                group.heading_galley.clone(),
                palette.group_heading,
            );
            y += group.heading_galley.rect.height() + GROUP_HEADING_GAP;

            for (row_index, row) in group.rows.iter().enumerate() {
                let resolved_row_height = row.action_layout.resolved_row_height;
                let row_rect = egui::Rect::from_min_size(
                    egui::pos2(column_rect.left(), y),
                    egui::vec2(column_rect.width(), resolved_row_height),
                );
                let combo_rect = egui::Rect::from_min_size(
                    row_rect.min,
                    egui::vec2(prepared.layout.target_combo_width, resolved_row_height),
                );
                let action_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        combo_rect.right() + row_item_spacing + style.action_gap,
                        row_rect.top(),
                    ),
                    egui::vec2(prepared.layout.action_width, resolved_row_height),
                );
                let combo_response = ui.interact(
                    combo_rect,
                    ui.id().with(("shortcut_combo", group_index, row_index)),
                    egui::Sense::hover(),
                );
                let action_response = ui.interact(
                    action_rect,
                    ui.id().with(("shortcut_action", group_index, row_index)),
                    egui::Sense::hover(),
                );
                #[cfg(test)]
                let combo_response_id = combo_response.id;
                #[cfg(test)]
                let action_response_id = action_response.id;
                let combo_overflowed =
                    paint_prepared_keycap_combo(ui, combo_rect, &row.combo_run, style);
                ui.painter().with_clip_rect(action_rect).galley(
                    action_rect.min + row.action_layout.galley_rect.min.to_vec2(),
                    row.action_layout.galley.clone(),
                    palette.text,
                );
                if let Some(full_text) = overflow_tooltip_text(&row.entry.combo, combo_overflowed) {
                    combo_response.on_hover_text(full_text);
                }
                if let Some(full_text) =
                    overflow_tooltip_text(&row.entry.action, row.action_layout.elided)
                {
                    action_response.on_hover_text(full_text);
                }

                #[cfg(test)]
                report.rows.push(ShortcutRowRenderReport {
                    group_index,
                    column_index,
                    combo: row.entry.combo.clone(),
                    action: row.entry.action.clone(),
                    row_rect,
                    combo_rect,
                    action_rect,
                    combo_run_width: row.combo_run.measured_width,
                    resolved_row_height,
                    combo_overflowed,
                    action_overflowed: row.action_layout.elided,
                    action_galley_rows: row.action_layout.galley.rows.len(),
                    action_max_rows: row.action_layout.galley.job.wrap.max_rows,
                    combo_response_id,
                    action_response_id,
                });
                y += resolved_row_height;
            }
        }
    }

    #[cfg(test)]
    {
        *report_slot = Some(report);
    }
}

fn combo_keycap_parts(combo: &str) -> Vec<&str> {
    combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn grouped_shortcuts(shortcuts: &[ShortcutEntry]) -> Vec<(String, Vec<&ShortcutEntry>)> {
    let mut grouped: Vec<(String, Vec<&ShortcutEntry>)> = Vec::new();
    let mut indexes: BTreeMap<String, usize> = BTreeMap::new();
    for entry in shortcuts {
        let group = entry.group.trim().to_owned();
        if let Some(index) = indexes.get(&group) {
            grouped[*index].1.push(entry);
        } else {
            indexes.insert(group.clone(), grouped.len());
            grouped.push((group, vec![entry]));
        }
    }
    grouped
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
    let restart_item = MenuItem::with_id(TRAY_MENU_RESTART, "재시작", true, None);
    let close_item = MenuItem::with_id(TRAY_MENU_CLOSE, "닫기", true, None);
    let menu = Menu::with_items(&[&settings_item, &restart_item, &close_item])?;
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

fn tray_action_for_menu_id(menu_id: &str) -> Option<TrayMenuAction> {
    match menu_id {
        TRAY_MENU_SETTINGS => Some(TrayMenuAction::Settings),
        TRAY_MENU_RESTART => Some(TrayMenuAction::Restart),
        TRAY_MENU_CLOSE => Some(TrayMenuAction::Close),
        _ => None,
    }
}

fn restart_application() -> Result<()> {
    let exe = env::current_exe().context("현재 실행 파일 경로를 확인하지 못했습니다")?;
    Command::new(&exe)
        .spawn()
        .with_context(|| format!("새 프로세스를 시작하지 못했습니다: {}", exe.display()))?;
    Ok(())
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

fn group_heading_font_id(size: f32) -> egui::FontId {
    egui::FontId::new(
        size,
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

fn apply_view_visuals(visuals: &mut egui::Visuals, view: AppView) {
    match view {
        AppView::Shortcuts => {
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            visuals.window_fill = egui::Color32::TRANSPARENT;
        }
    }
}

impl eframe::App for CheatSheetsApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.remember_repaint_context(ctx);
        self.poll_hotkey(ctx);
        self.poll_tray(ctx);
        self.persist_window_settings_if_changed(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.remember_repaint_context(&ctx);
        configure_style(&ctx, self.settings.theme);
        let theme = resolved_theme(&ctx, self.settings.theme);
        let palette = palette_for(theme);
        self.show_settings_popup(&ctx, palette);
        if !self.visible {
            return;
        }

        if self.capture_target.is_none() && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.visible = false;
            if should_os_hide_root_overlay(self.settings_popup_open) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                self.settings_popup_needs_focus = true;
                ctx.request_repaint();
            }
            return;
        }

        {
            let visuals = ui.visuals_mut();
            apply_palette_to_visuals(visuals, palette);
            apply_view_visuals(visuals, self.view);
        }

        apply_viewport_chrome(&ctx, self.view);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(ui.max_rect().shrink(view_content_inset(self.view)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_min_size(ui.available_size());
                self.show_shortcuts(ui);
            },
        );
    }

    fn on_exit(&mut self) {
        let _ = storage::save_app_settings(&self.settings);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        transparent_clear_color()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    #[allow(deprecated)]
    fn with_font_measurement_ui(run_ui: impl FnOnce(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        let mut run_ui = Some(run_ui);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                run_ui
                    .take()
                    .expect("measurement UI should run exactly once")(ui);
            });
        });
    }

    #[allow(deprecated)]
    fn render_shortcut_columns_frame(
        ctx: &egui::Context,
        entry: &ShortcutEntry,
        style: ResolvedOverlayStyle,
        pointer_pos: Option<egui::Pos2>,
        time: f64,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 300.0),
            )),
            time: Some(time),
            ..Default::default()
        };
        if let Some(pointer_pos) = pointer_pos {
            input.events.push(egui::Event::PointerMoved(pointer_pos));
        }

        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show_shortcut_columns(ui, std::slice::from_ref(entry), style);
            });
        })
    }

    fn test_shortcut_action_layout(
        ui: &egui::Ui,
        action: &str,
        action_width: f32,
        max_rows: usize,
        style: ResolvedOverlayStyle,
    ) -> ShortcutActionLayout {
        layout_shortcut_action(
            ui,
            action,
            action_width,
            max_rows,
            egui::FontId::proportional(style.action_text_size),
            style.palette.text,
            style.row_height,
            style.keycap_height,
            style.action_text_y_offset,
        )
    }

    fn assert_action_layout_inside_clip(layout: &ShortcutActionLayout, action_width: f32) {
        let action_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(action_width, layout.resolved_row_height),
        );

        assert!(
            action_rect.contains_rect(layout.galley_rect),
            "shifted galley {:?} must stay inside action rect {:?}",
            layout.galley_rect,
            action_rect
        );
    }

    fn rendered_action_clip(
        output: &egui::FullOutput,
        action: &str,
        action_color: egui::Color32,
    ) -> egui::Rect {
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape)
                    if shape.galley.job.text == action && shape.fallback_color == action_color =>
                {
                    Some(clipped.clip_rect)
                }
                _ => None,
            })
            .expect("production renderer should paint the shortcut action")
    }

    fn legal_ordered_partitions(
        group_count: usize,
        column_count: usize,
    ) -> Vec<Vec<std::ops::Range<usize>>> {
        fn enumerate_boundaries(
            group_count: usize,
            column_count: usize,
            next_boundary: usize,
            boundaries: &mut Vec<usize>,
            partitions: &mut Vec<Vec<std::ops::Range<usize>>>,
        ) {
            if boundaries.len() == column_count - 1 {
                let mut starts = Vec::with_capacity(column_count + 1);
                starts.push(0);
                starts.extend(boundaries.iter().copied());
                starts.push(group_count);
                partitions.push(starts.windows(2).map(|pair| pair[0]..pair[1]).collect());
                return;
            }

            let remaining_boundaries = column_count - 1 - boundaries.len();
            let last_boundary = group_count - remaining_boundaries;
            for boundary in next_boundary..=last_boundary {
                boundaries.push(boundary);
                enumerate_boundaries(
                    group_count,
                    column_count,
                    boundary + 1,
                    boundaries,
                    partitions,
                );
                boundaries.pop();
            }
        }

        let mut partitions = Vec::new();
        enumerate_boundaries(
            group_count,
            column_count,
            1,
            &mut Vec::new(),
            &mut partitions,
        );
        partitions
    }

    fn test_range_height(
        group_block_heights: &[f32],
        inter_group_gap: f32,
        range: &std::ops::Range<usize>,
    ) -> f32 {
        group_block_heights[range.clone()].iter().sum::<f32>()
            + range.len().saturating_sub(1) as f32 * inter_group_gap
    }

    fn test_partition_height(
        group_block_heights: &[f32],
        inter_group_gap: f32,
        ranges: &[std::ops::Range<usize>],
    ) -> f32 {
        ranges
            .iter()
            .map(|range| test_range_height(group_block_heights, inter_group_gap, range))
            .max_by(f32::total_cmp)
            .expect("a partition must contain at least one range")
    }

    #[test]
    fn ordered_partition_preserves_every_group_once() {
        let ranges = partition_group_ranges(&[40.0, 10.0, 35.0, 15.0, 20.0], 6.0, 3);

        assert_eq!(ranges.len(), 3);
        assert!(ranges.iter().all(|range| !range.is_empty()));
        assert_eq!(ranges.first().expect("first range").start, 0);
        assert_eq!(ranges.last().expect("last range").end, 5);
        assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
        assert_eq!(
            ranges
                .iter()
                .flat_map(|range| range.clone())
                .collect::<Vec<_>>(),
            (0..5).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ordered_partition_matches_bruteforce_optimum_for_small_inputs() {
        let cases = [
            (vec![100.0, 20.0, 20.0, 20.0], 0.0, 2),
            (vec![8.0, 7.0, 6.0, 5.0, 4.0], 2.0, 3),
            (vec![5.0, 13.0, 2.0, 11.0], 3.5, 2),
            (vec![9.0, 1.0, 4.0], 1.0, 3),
        ];

        for (heights, gap, columns) in cases {
            let actual = partition_group_ranges(&heights, gap, columns);
            let actual_height = test_partition_height(&heights, gap, &actual);
            let optimum = legal_ordered_partitions(heights.len(), columns)
                .iter()
                .map(|ranges| test_partition_height(&heights, gap, ranges))
                .min_by(f32::total_cmp)
                .expect("at least one legal partition");

            assert_eq!(actual_height, optimum, "heights={heights:?}, gap={gap}");
        }
    }

    #[test]
    fn ordered_partition_uses_lexicographically_earliest_boundaries_on_ties() {
        assert_eq!(
            partition_group_ranges(&[1.0, 1.0, 1.0], 0.0, 2),
            vec![0..1, 1..3]
        );
    }

    #[test]
    fn ordered_partition_excludes_trailing_inter_group_gap() {
        assert_eq!(group_range_height(&[10.0, 20.0], 7.0, 0..2), 37.0);
        assert_eq!(group_range_height(&[10.0, 20.0], 7.0, 1..2), 20.0);
    }

    fn canonical_adaptive_shortcuts() -> Vec<ShortcutEntry> {
        vec![
            ShortcutEntry {
                combo: "Ctrl+Shift+P".to_owned(),
                action: "워크스페이스 전체 명령 팔레트를 열고 실행 가능한 작업을 빠르게 검색합니다".to_owned(),
                group: "탐색".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+P".to_owned(),
                action: "파일 이름과 상대 경로를 기준으로 현재 프로젝트의 문서를 즉시 찾습니다".to_owned(),
                group: "탐색".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Shift+O".to_owned(),
                action: "현재 파일의 심볼 목록을 열어 함수와 타입 정의로 이동합니다".to_owned(),
                group: "탐색".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Alt+Left".to_owned(),
                action: "이전에 확인한 편집 위치로 돌아가 탐색 흐름을 계속 이어갑니다".to_owned(),
                group: "탐색".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Shift+K".to_owned(),
                action: "현재 선택 영역과 연결된 줄을 삭제하고 다음 편집 위치를 유지합니다".to_owned(),
                group: "편집".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Shift+B".to_owned(),
                action: "현재 작업의 기본 빌드 태스크를 실행하고 출력 패널에서 결과를 확인합니다".to_owned(),
                group: "실행".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+F5".to_owned(),
                action: "디버거 연결 없이 현재 프로젝트를 실행하고 종료 상태를 확인합니다".to_owned(),
                group: "실행".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Shift+Alt+Super+PageDown".to_owned(),
                action: "선택한 자동화 세션의 마지막 실행 단계와 상세 이벤트 로그로 이동합니다".to_owned(),
                group: "실행".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Alt+N".to_owned(),
                action: "새 작업 세션을 열고 현재 워크스페이스 컨텍스트를 그대로 이어받습니다".to_owned(),
                group: "세션".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Alt+W".to_owned(),
                action: "활성 세션을 안전하게 닫고 아직 저장하지 않은 변경을 먼저 확인합니다".to_owned(),
                group: "세션".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+Shift+G".to_owned(),
                action: "소스 제어 변경 목록을 열고 파일별 diff와 스테이징 상태를 검토합니다".to_owned(),
                group: "검토".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "F8".to_owned(),
                action: "현재 파일에서 다음 진단 오류 또는 경고 위치로 이동합니다".to_owned(),
                group: "검토".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Shift+F8".to_owned(),
                action: "현재 파일에서 이전 진단 오류 또는 경고 위치로 이동합니다".to_owned(),
                group: "검토".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+K+Ctrl+X".to_owned(),
                action: r"C:\workspace\packages\adaptive_overlay\generated\artifacts\verification\report.json".to_owned(),
                group: "검토".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "F1".to_owned(),
                action: "현재 화면에서 사용할 수 있는 명령과 관련 도움말 문서를 함께 엽니다".to_owned(),
                group: "도움말".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
            ShortcutEntry {
                combo: "Ctrl+K+Ctrl+S".to_owned(),
                action: "단축키 편집기를 열어 현재 키 조합의 충돌과 적용 범위를 확인합니다".to_owned(),
                group: "도움말".to_owned(),
                source: crate::core::ShortcutSource::User,
            },
        ]
    }

    fn canonical_adaptive_style() -> ResolvedOverlayStyle {
        resolved_overlay_style(
            &OverlayStyleSettings {
                card_padding: 20.0,
                group_heading_size: 13.0,
                action_text_size: 12.0,
                action_text_y_offset: -0.75,
                keycap_text_size: 9.5,
                row_height: 18.0,
                combo_width: 170.0,
                action_gap: 0.0,
                keycap_height: 16.0,
                keycap_gap: 3.0,
                ..Default::default()
            },
            0.96,
        )
    }

    #[allow(deprecated)]
    fn render_adaptive_overlay_frame(
        ctx: &egui::Context,
        shortcuts: &[ShortcutEntry],
        style: ResolvedOverlayStyle,
        inner_width: f32,
        window_height: f32,
        pointer_pos: Option<egui::Pos2>,
        time: f64,
    ) -> (egui::FullOutput, ShortcutRenderReport) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(inner_width, window_height),
            )),
            time: Some(time),
            ..Default::default()
        };
        if let Some(pointer_pos) = pointer_pos {
            input.events.push(egui::Event::PointerMoved(pointer_pos));
        }

        let mut report = None;
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                    render_shortcut_columns(ui, shortcuts, style, &mut report);
                });
        });
        (
            output,
            report.expect("production shortcut renderer should return a report"),
        )
    }

    fn rects_are_close(left: egui::Rect, right: egui::Rect) -> bool {
        const EPSILON: f32 = 0.01;
        (left.left() - right.left()).abs() < EPSILON
            && (left.top() - right.top()).abs() < EPSILON
            && (left.right() - right.right()).abs() < EPSILON
            && (left.bottom() - right.bottom()).abs() < EPSILON
    }

    #[test]
    fn production_shortcut_renderer_returns_no_report() {
        let shortcuts = canonical_adaptive_shortcuts();
        let style = canonical_adaptive_style();

        with_font_measurement_ui(|ui| {
            let _: () = show_shortcut_columns(ui, &shortcuts, style);
        });
    }

    #[test]
    fn renderer_consumes_canonical_prepared_layout() {
        let shortcuts = canonical_adaptive_shortcuts();
        let style = canonical_adaptive_style();
        let wide_ctx = egui::Context::default();
        install_korean_font(&wide_ctx).expect("production Korean font installation should succeed");

        let (_, wide) =
            render_adaptive_overlay_frame(&wide_ctx, &shortcuts, style, 1_715.0, 873.0, None, 0.0);

        assert_eq!(wide.layout.target_action_width, 320.0);
        assert_eq!(wide.layout.target_combo_width, 170.0);
        assert_eq!(wide.layout.column_count, 3);
        assert_eq!(wide.group_ranges, vec![0..2, 2..4, 4..6]);
        assert_eq!(wide.row_item_spacing, 12.0);
        assert_eq!(wide.column_gap, 12.0);
        assert_eq!(wide.heading_gap, 6.0);
        assert_eq!(wide.inter_group_gap, 16.0);
        assert_eq!(wide.column_rects.len(), 3);
        assert!(wide.rows.iter().all(|row| row.action_max_rows == 1));
        assert!(wide.rows.iter().all(|row| row.action_galley_rows == 1));

        for group_index in 0..wide.group_block_heights.len() {
            let expected = wide.group_heading_heights[group_index]
                + wide.heading_gap
                + wide.group_row_heights[group_index].iter().sum::<f32>();
            assert!((wide.group_block_heights[group_index] - expected).abs() < 0.01);
        }
        for row in &wide.rows {
            assert!(wide.column_rects[row.column_index].contains_rect(row.row_rect));
            assert!(row.row_rect.contains_rect(row.combo_rect));
            assert!(row.row_rect.contains_rect(row.action_rect));
        }

        let narrow_ctx = egui::Context::default();
        install_korean_font(&narrow_ctx)
            .expect("production Korean font installation should succeed");
        let (_, narrow) =
            render_adaptive_overlay_frame(&narrow_ctx, &shortcuts, style, 880.0, 560.0, None, 0.0);
        assert_eq!(narrow.layout.column_count, 1);
        assert_eq!(narrow.group_ranges, vec![0..6]);
    }

    #[test]
    fn renderer_shapes_use_column_action_and_combo_clip_rects() {
        let shortcuts = canonical_adaptive_shortcuts();
        let style = canonical_adaptive_style();
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        let (output, report) =
            render_adaptive_overlay_frame(&ctx, &shortcuts, style, 1_715.0, 873.0, None, 0.0);

        for row in &report.rows {
            let action_shape = output
                .shapes
                .iter()
                .find(|clipped| {
                    matches!(
                        &clipped.shape,
                        egui::Shape::Text(shape)
                            if shape.galley.job.text == row.action
                                && shape.fallback_color == style.palette.text
                    )
                })
                .expect("every prepared action should be painted");
            assert!(rects_are_close(action_shape.clip_rect, row.action_rect));

            let combo_shapes = output
                .shapes
                .iter()
                .filter(|clipped| rects_are_close(clipped.clip_rect, row.combo_rect))
                .collect::<Vec<_>>();
            assert!(combo_shapes.iter().any(|clipped| matches!(
                &clipped.shape,
                egui::Shape::Rect(shape) if shape.fill == style.palette.keycap_fill
            )));
            assert!(combo_shapes.iter().any(|clipped| matches!(
                &clipped.shape,
                egui::Shape::Rect(shape) if shape.stroke.color == style.palette.keycap_border
            )));
            assert!(combo_shapes.iter().any(|clipped| matches!(
                &clipped.shape,
                egui::Shape::Text(shape) if shape.fallback_color == style.palette.keycap_text
            )));
        }

        for clipped in &output.shapes {
            let is_keycap_shape = match &clipped.shape {
                egui::Shape::Rect(shape) => {
                    shape.fill == style.palette.keycap_fill
                        || shape.stroke.color == style.palette.keycap_border
                }
                egui::Shape::Text(shape) => shape.fallback_color == style.palette.keycap_text,
                _ => false,
            };
            if is_keycap_shape {
                assert!(
                    report
                        .rows
                        .iter()
                        .any(|row| rects_are_close(clipped.clip_rect, row.combo_rect)),
                    "keycap shape must use its row combo clip: {:?}",
                    clipped.clip_rect
                );
            }
        }

        for (group_index, heading_height) in report.group_heading_heights.iter().enumerate() {
            assert!(*heading_height > 0.0);
            let group = &report.group_names[group_index];
            let heading_shape = output
                .shapes
                .iter()
                .find(|clipped| {
                    matches!(
                        &clipped.shape,
                        egui::Shape::Text(shape)
                            if shape.galley.job.text == *group
                                && shape.fallback_color == style.palette.group_heading
                    )
                })
                .expect("every group heading should be painted");
            let column_index = report
                .group_ranges
                .iter()
                .position(|range| range.contains(&group_index))
                .expect("every group must belong to a column");
            assert!(rects_are_close(
                heading_shape.clip_rect,
                report.column_rects[column_index]
            ));
        }
    }

    #[test]
    fn renderer_hover_shows_exact_full_action_and_combo() {
        const PATH_ACTION: &str =
            r"C:\workspace\packages\adaptive_overlay\generated\artifacts\verification\report.json";
        const LONG_COMBO: &str = "Ctrl+Shift+Alt+Super+PageDown";
        let shortcuts = canonical_adaptive_shortcuts();
        let style = canonical_adaptive_style();

        for (probe, is_action) in [(PATH_ACTION, true), (LONG_COMBO, false)] {
            let ctx = egui::Context::default();
            install_korean_font(&ctx).expect("production Korean font installation should succeed");
            ctx.global_style_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let (_, first_report) =
                render_adaptive_overlay_frame(&ctx, &shortcuts, style, 1_715.0, 873.0, None, 0.0);
            let first_row = first_report
                .rows
                .iter()
                .find(|row| {
                    if is_action {
                        row.action == probe
                    } else {
                        row.combo == probe
                    }
                })
                .expect("canonical overflow probe must be reported");
            assert!(if is_action {
                first_row.action_overflowed
            } else {
                first_row.combo_overflowed
            });
            let hover_rect = if is_action {
                first_row.action_rect
            } else {
                first_row.combo_rect
            };
            let response_id = if is_action {
                first_row.action_response_id
            } else {
                first_row.combo_response_id
            };

            let (_, hover_report) = render_adaptive_overlay_frame(
                &ctx,
                &shortcuts,
                style,
                1_715.0,
                873.0,
                Some(hover_rect.center()),
                1.0,
            );
            let hover_row = hover_report
                .rows
                .iter()
                .find(|row| {
                    if is_action {
                        row.action == probe
                    } else {
                        row.combo == probe
                    }
                })
                .expect("hovered overflow probe must remain reported");
            assert_eq!(
                response_id,
                if is_action {
                    hover_row.action_response_id
                } else {
                    hover_row.combo_response_id
                }
            );

            let (hovered_output, _) =
                render_adaptive_overlay_frame(&ctx, &shortcuts, style, 1_715.0, 873.0, None, 2.0);
            assert!(hovered_output.shapes.iter().any(|clipped| {
                !rects_are_close(clipped.clip_rect, hover_rect)
                    && matches!(
                        &clipped.shape,
                        egui::Shape::Text(shape) if shape.galley.job.text == probe
                    )
            }));
        }
    }

    #[test]
    fn renderer_maximum_style_remains_contained() {
        let shortcuts = canonical_adaptive_shortcuts();
        for action_text_y_offset in [-6.0, 6.0] {
            let style = resolved_overlay_style(
                &OverlayStyleSettings {
                    card_padding: 64.0,
                    group_heading_size: 24.0,
                    action_text_size: 24.0,
                    action_text_y_offset,
                    keycap_text_size: 18.0,
                    row_height: 40.0,
                    combo_width: 220.0,
                    action_gap: 32.0,
                    keycap_height: 28.0,
                    keycap_gap: 16.0,
                    ..Default::default()
                },
                0.96,
            );
            let minimum_content_width = WindowPlacement::MIN_WIDTH - 2.0 * style.card_padding;
            let wide_ctx = egui::Context::default();
            install_korean_font(&wide_ctx)
                .expect("production Korean font installation should succeed");
            let (wide_output, wide) = render_adaptive_overlay_frame(
                &wide_ctx, &shortcuts, style, 1_715.0, 873.0, None, 0.0,
            );

            assert_eq!(wide.layout.column_count, 2);
            assert!(wide.layout.action_width >= wide.layout.target_action_width);
            for row in &wide.rows {
                assert!(wide.column_rects[row.column_index].contains_rect(row.row_rect));
                assert!(row.row_rect.contains_rect(row.action_rect));
                assert!(row.row_rect.contains_rect(row.combo_rect));
                let action_shape = wide_output
                    .shapes
                    .iter()
                    .find(|clipped| {
                        matches!(
                            &clipped.shape,
                            egui::Shape::Text(shape)
                                if shape.galley.job.text == row.action
                                    && shape.fallback_color == style.palette.text
                        )
                    })
                    .expect("maximum style action should be painted");
                assert!(rects_are_close(action_shape.clip_rect, row.action_rect));
            }

            let minimum_ctx = egui::Context::default();
            install_korean_font(&minimum_ctx)
                .expect("production Korean font installation should succeed");
            let (_, minimum) = render_adaptive_overlay_frame(
                &minimum_ctx,
                &shortcuts,
                style,
                minimum_content_width,
                WindowPlacement::MIN_HEIGHT,
                None,
                0.0,
            );

            assert_eq!(minimum_content_width, 792.0);
            assert_eq!(minimum.layout.column_count, 1);
            assert!(minimum.layout.action_width >= minimum.layout.target_action_width);
            assert!(minimum.rows.iter().all(|row| row.action_max_rows == 2));
            for row in &minimum.rows {
                assert!(minimum.column_rects[row.column_index].contains_rect(row.row_rect));
                assert!(row.row_rect.contains_rect(row.action_rect));
                assert!(row.row_rect.contains_rect(row.combo_rect));
            }
        }
    }

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

        assert_eq!(
            labels,
            vec!["일반", "오버레이 표시", "단축키", "단축키 편집"]
        );
    }

    #[test]
    fn settings_section_descriptions_are_not_empty() {
        for section in settings_sections() {
            assert!(!section.description().is_empty());
        }
    }

    #[test]
    fn settings_action_opens_popup_and_requests_focus() {
        let mut open = false;
        let mut needs_focus = false;

        open_settings_popup_state(&mut open, &mut needs_focus);

        assert!(open);
        assert!(needs_focus);
    }

    #[test]
    fn settings_popup_close_request_closes_popup() {
        let mut open = true;
        let mut capture = None;
        close_settings_popup_state(&mut open, &mut capture);
        assert!(!open);
    }

    #[test]
    fn closing_settings_popup_clears_active_capture() {
        let mut open = true;
        let mut capture = Some(CaptureTarget::ToggleHotkey);

        close_settings_popup_state(&mut open, &mut capture);

        assert!(!open);
        assert_eq!(capture, None);
    }

    #[test]
    fn settings_popup_escape_does_not_close_while_capturing_shortcut() {
        assert!(!settings_popup_should_close_on_escape(
            Some(CaptureTarget::ShortcutCombo),
            true
        ));
    }

    #[test]
    fn settings_popup_escape_closes_when_not_capturing_shortcut() {
        assert!(settings_popup_should_close_on_escape(None, true));
    }

    #[test]
    fn overlay_hide_is_deferred_while_settings_popup_is_open() {
        assert!(!should_os_hide_root_overlay(true));
        assert!(should_os_hide_root_overlay(false));
    }

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
    fn combo_parts_trim_empty_segments() {
        assert_eq!(
            combo_keycap_parts(" Ctrl + Shift + P "),
            vec!["Ctrl", "Shift", "P"]
        );
    }

    #[test]
    fn measurement_context_uses_installed_korean_font() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);

            assert!(ui.ctx().fonts_mut(|fonts| {
                fonts.has_glyph(&egui::FontId::monospace(style.keycap_text_size), '한')
            }));
            assert!(ui.ctx().fonts_mut(|fonts| {
                fonts.has_glyph(&egui::FontId::proportional(style.action_text_size), '글')
            }));
        });
    }

    #[test]
    fn keycap_width_tracks_resolved_font_metrics() {
        with_font_measurement_ui(|ui| {
            let default_style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let maximum_style = resolved_overlay_style(
                &OverlayStyleSettings {
                    keycap_text_size: 18.0,
                    ..Default::default()
                },
                0.96,
            );

            let one_glyph = measure_keycap_label_width(ui, "한", default_style);
            let longer_label = measure_keycap_label_width(ui, "한글키캡", default_style);
            let maximum_size = measure_keycap_label_width(ui, "한글키캡", maximum_style);
            let raw_label_width = ui
                .painter()
                .layout_no_wrap(
                    "한글키캡".to_owned(),
                    egui::FontId::monospace(default_style.keycap_text_size),
                    default_style.palette.keycap_text,
                )
                .size()
                .x;

            assert!(longer_label > one_glyph);
            assert!(maximum_size > longer_label);
            assert!((longer_label - (raw_label_width + 9.0).max(16.0)).abs() < 0.01);
            assert_eq!(measure_keycap_label_width(ui, "", default_style), 16.0);
        });
    }

    #[test]
    fn combo_run_width_includes_configured_keycap_gap() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(
                &OverlayStyleSettings {
                    keycap_gap: 11.0,
                    ..Default::default()
                },
                0.96,
            );
            let parts = ["Ctrl", "Shift", "명령"];
            let labels_width = parts
                .iter()
                .map(|part| measure_keycap_label_width(ui, part, style))
                .sum::<f32>();
            let run_width = measure_combo_run_width(ui, &parts, style);

            assert!((run_width - labels_width - style.keycap_gap * 2.0).abs() < 0.01);
        });
    }

    #[test]
    fn action_width_uses_resolved_proportional_font() {
        with_font_measurement_ui(|ui| {
            let default_style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let maximum_style = resolved_overlay_style(
                &OverlayStyleSettings {
                    action_text_size: 24.0,
                    ..Default::default()
                },
                0.96,
            );

            let one_glyph = measure_action_width(ui, "한", default_style);
            let longer_label = measure_action_width(ui, "한글 설명 문구", default_style);
            let maximum_size = measure_action_width(ui, "한글 설명 문구", maximum_style);

            assert!(longer_label > one_glyph);
            assert!(maximum_size > longer_label);
        });
    }

    #[test]
    fn multi_column_action_uses_one_row_and_elides() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action = "워크스페이스 전체 명령 팔레트를 열고 실행 가능한 작업을 검색합니다";
            let action_width = 120.0;
            let layout = test_shortcut_action_layout(ui, action, action_width, 1, style);

            assert_eq!(layout.galley.job.wrap.max_width, action_width);
            assert_eq!(layout.galley.job.wrap.max_rows, 1);
            assert!(layout.galley.job.wrap.break_anywhere);
            assert_eq!(layout.galley.job.wrap.overflow_character, Some('…'));
            assert_eq!(layout.galley.rows.len(), 1);
            assert!(layout.elided);
            assert_action_layout_inside_clip(&layout, action_width);
        });
    }

    #[test]
    fn unbroken_action_stays_inside_available_width() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action = "workspace/src/features/shortcut_overlay/extremely_long_command_name.rs";
            let action_width = 96.0;
            let layout = test_shortcut_action_layout(ui, action, action_width, 1, style);

            assert!(layout.elided);
            assert!(layout.galley_rect.width() <= action_width);
            assert_action_layout_inside_clip(&layout, action_width);
        });
    }

    #[test]
    fn non_elided_action_does_not_request_tooltip() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action = "파일 열기";
            let layout = test_shortcut_action_layout(ui, action, 240.0, 1, style);

            assert!(!layout.elided);
            assert_eq!(overflow_tooltip_text(action, layout.elided), None);
        });
    }

    #[test]
    fn elided_action_requests_full_text_tooltip() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action = "현재 Project의 모든 파일과 Korean symbol을 빠르게 검색합니다";
            let layout = test_shortcut_action_layout(ui, action, 110.0, 1, style);

            assert!(layout.elided);
            assert_eq!(overflow_tooltip_text(action, layout.elided), Some(action));
        });
    }

    #[test]
    fn action_row_contains_positive_and_negative_optical_offsets() {
        with_font_measurement_ui(|ui| {
            for offset in [-6.0, 6.0] {
                let style = resolved_overlay_style(
                    &OverlayStyleSettings {
                        action_text_y_offset: offset,
                        ..Default::default()
                    },
                    0.96,
                );
                let layout = test_shortcut_action_layout(ui, "한글 Action", 240.0, 1, style);

                assert!(
                    layout.resolved_row_height >= layout.galley.rect.height() + 2.0 * offset.abs()
                );
                assert!(layout.resolved_row_height >= style.row_height);
                assert!(layout.resolved_row_height >= style.keycap_height);
                assert_action_layout_inside_clip(&layout, 240.0);
            }
        });
    }

    #[test]
    fn single_column_action_uses_at_most_two_rows() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action = "현재 파일의 모든 symbol과 참조 위치를 찾아 두 줄을 넘는 상세 결과까지 빠르게 확인합니다";
            let action_width = 150.0;
            let layout = test_shortcut_action_layout(ui, action, action_width, 2, style);

            assert_eq!(layout.galley.job.wrap.max_rows, 2);
            assert_eq!(layout.galley.rows.len(), 2);
            assert!(layout.elided);
            assert_action_layout_inside_clip(&layout, action_width);
        });
    }

    #[test]
    fn single_column_action_height_expands_for_second_row() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let action =
                "선택한 파일의 변경 내용을 비교하고 이전 revision과 현재 상태를 함께 표시합니다";
            let action_width = 165.0;
            let layout = test_shortcut_action_layout(ui, action, action_width, 2, style);

            assert_eq!(layout.galley.rows.len(), 2);
            assert!(layout.resolved_row_height > style.row_height);
            assert!(
                layout.resolved_row_height
                    >= layout.galley.rect.height() + 2.0 * style.action_text_y_offset.abs()
            );
            assert_action_layout_inside_clip(&layout, action_width);
        });
    }

    #[test]
    fn single_column_row_height_honors_configured_minimum() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(
                &OverlayStyleSettings {
                    row_height: 48.0,
                    ..Default::default()
                },
                0.96,
            );
            let layout = test_shortcut_action_layout(ui, "설정 열기", 240.0, 2, style);

            assert!(layout.resolved_row_height >= style.row_height);
            assert!(layout.resolved_row_height >= style.keycap_height);
            assert!(
                layout.resolved_row_height
                    >= layout.galley.rect.height() + 2.0 * style.action_text_y_offset.abs()
            );
            assert_action_layout_inside_clip(&layout, 240.0);
        });
    }

    #[test]
    fn production_action_shape_uses_exact_action_clip() {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
        let action = "워크스페이스 전체 명령 팔레트를 열고 실행 가능한 작업을 빠르게 검색합니다";
        let entry = ShortcutEntry {
            combo: "Ctrl+Shift+P".to_owned(),
            action: action.to_owned(),
            group: "탐색".to_owned(),
            source: crate::core::ShortcutSource::User,
        };

        let (output, report) = render_adaptive_overlay_frame(
            &ctx,
            std::slice::from_ref(&entry),
            style,
            800.0,
            300.0,
            None,
            0.0,
        );
        let combo_clip = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == style.palette.keycap_fill => {
                    Some(clipped.clip_rect)
                }
                _ => None,
            })
            .expect("production renderer should paint the combo rail");
        let action_clip = rendered_action_clip(&output, action, style.palette.text);
        let action_position = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape)
                    if shape.galley.job.text == action
                        && shape.fallback_color == style.palette.text =>
                {
                    Some(shape.pos)
                }
                _ => None,
            })
            .expect("production renderer should paint the action galley");
        let row = report
            .rows
            .first()
            .expect("production renderer should report the action row");
        let expected_action_rect = egui::Rect::from_min_max(
            egui::pos2(
                combo_clip.right() + report.row_item_spacing + style.action_gap,
                combo_clip.top(),
            ),
            egui::pos2(report.column_rects[0].right(), combo_clip.bottom()),
        );

        assert!((action_clip.left() - action_position.x).abs() < 0.01);
        assert!(rects_are_close(combo_clip, row.combo_rect));
        assert!(
            rects_are_close(action_clip, expected_action_rect),
            "action clip {action_clip:?} must equal expected rect {expected_action_rect:?}"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn single_column_action_renderer_uses_two_rows() {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
        let action = "현재 파일의 모든 심볼과 참조 위치를 찾아 변경 이력 및 관련 명령을 두 줄 범위에서 확인하고 나머지는 생략합니다";
        let entry = ShortcutEntry {
            combo: "Ctrl+Shift+O".to_owned(),
            action: action.to_owned(),
            group: "탐색".to_owned(),
            source: crate::core::ShortcutSource::User,
        };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(460.0, 300.0),
            )),
            ..Default::default()
        };

        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show_shortcut_columns(ui, std::slice::from_ref(&entry), style);
            });
        });
        let action_shape = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape)
                    if shape.galley.job.text == action
                        && shape.fallback_color == style.palette.text =>
                {
                    Some(shape)
                }
                _ => None,
            })
            .expect("single-column renderer should paint the action galley");

        assert_eq!(action_shape.galley.job.wrap.max_rows, 2);
        assert_eq!(action_shape.galley.rows.len(), 2);
        assert!(action_shape.galley.elided);
    }

    #[test]
    fn action_response_tooltip_uses_exact_full_text_only_when_elided() {
        let long_ctx = egui::Context::default();
        install_korean_font(&long_ctx).expect("production Korean font installation should succeed");
        long_ctx.global_style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
        let long_action = "현재 Project의 모든 파일과 Korean symbol을 빠르게 검색하고 결과 위치로 이동한 뒤 연결된 참조, 변경 이력, 진단 메시지, 실행 로그를 차례로 확인하고 관련 파일의 상세 diff와 적용 범위를 비교하여 현재 작업에 필요한 후속 명령을 선택하고 검토가 끝나면 원래 편집 위치로 안전하게 돌아옵니다";
        let long_entry = ShortcutEntry {
            combo: "Ctrl+Shift+F".to_owned(),
            action: long_action.to_owned(),
            group: "탐색".to_owned(),
            source: crate::core::ShortcutSource::User,
        };

        let first_long_frame =
            render_shortcut_columns_frame(&long_ctx, &long_entry, style, None, 0.0);
        let long_action_clip =
            rendered_action_clip(&first_long_frame, long_action, style.palette.text);
        let first_action_shape = first_long_frame
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == long_action => Some(shape),
                _ => None,
            })
            .expect("long action shape should exist");
        assert!(first_action_shape.galley.elided);
        let _ = render_shortcut_columns_frame(
            &long_ctx,
            &long_entry,
            style,
            Some(long_action_clip.center()),
            1.0,
        );
        let hovered_long_frame =
            render_shortcut_columns_frame(&long_ctx, &long_entry, style, None, 2.0);

        assert!(hovered_long_frame.shapes.iter().any(|clipped| {
            clipped.clip_rect != long_action_clip
                && matches!(
                    &clipped.shape,
                    egui::Shape::Text(shape) if shape.galley.job.text == long_action
                )
        }));

        let short_ctx = egui::Context::default();
        install_korean_font(&short_ctx)
            .expect("production Korean font installation should succeed");
        short_ctx.global_style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let short_action = "파일 열기";
        let short_entry = ShortcutEntry {
            combo: "Ctrl+O".to_owned(),
            action: short_action.to_owned(),
            group: "탐색".to_owned(),
            source: crate::core::ShortcutSource::User,
        };

        let first_short_frame =
            render_shortcut_columns_frame(&short_ctx, &short_entry, style, None, 0.0);
        let short_action_clip =
            rendered_action_clip(&first_short_frame, short_action, style.palette.text);
        let _ = render_shortcut_columns_frame(
            &short_ctx,
            &short_entry,
            style,
            Some(short_action_clip.center()),
            1.0,
        );
        let hovered_short_frame =
            render_shortcut_columns_frame(&short_ctx, &short_entry, style, None, 2.0);

        assert!(!hovered_short_frame.shapes.iter().any(|clipped| {
            clipped.clip_rect != short_action_clip
                && matches!(
                    &clipped.shape,
                    egui::Shape::Text(shape) if shape.galley.job.text == short_action
                )
        }));
    }

    #[test]
    fn action_target_width_uses_nearest_rank_percentile_and_clamps() {
        let unsorted = [
            300.0, 100.0, 220.0, 180.0, 210.0, 200.0, 190.0, 230.0, 240.0, 250.0,
        ];

        assert_eq!(target_action_width(&unsorted), 250.0);
        assert_eq!(target_action_width(&[42.0]), 180.0);
        assert_eq!(target_action_width(&[500.0]), 320.0);
        assert_eq!(target_action_width(&[]), 180.0);
    }

    #[test]
    #[should_panic(expected = "action width measurements must be finite")]
    fn action_target_width_rejects_non_finite_measurements() {
        target_action_width(&[f32::NAN]);
    }

    #[test]
    fn combo_target_width_honors_configured_minimum_and_cap() {
        let unsorted = [
            100.0, 120.0, 80.0, 110.0, 200.0, 90.0, 105.0, 115.0, 95.0, 130.0,
        ];

        assert_eq!(target_combo_width(&unsorted, 112.0), 130.0);
        assert_eq!(target_combo_width(&[80.0], 140.0), 140.0);
        assert_eq!(target_combo_width(&[300.0], 112.0), 220.0);
        assert_eq!(target_combo_width(&[], 112.0), 112.0);
    }

    #[test]
    #[should_panic(expected = "combo width measurements must be finite")]
    fn combo_target_width_rejects_non_finite_measurements() {
        target_combo_width(&[f32::NAN], 112.0);
    }

    #[test]
    #[should_panic(expected = "configured combo width must be finite")]
    fn combo_target_width_rejects_non_finite_configured_width() {
        target_combo_width(&[112.0], f32::INFINITY);
    }

    #[test]
    fn adaptive_columns_never_exceed_group_count() {
        let two_groups = calculate_overlay_layout(4_000.0, 2, 112.0, 180.0, 12.0, 8.0, 12.0);
        let three_groups = calculate_overlay_layout(4_000.0, 3, 112.0, 180.0, 12.0, 8.0, 12.0);

        assert_eq!(two_groups.column_count, 2);
        assert_eq!(three_groups.column_count, 3);
    }

    #[test]
    fn one_group_layout_always_uses_one_column() {
        let layout = calculate_overlay_layout(4_000.0, 1, 112.0, 180.0, 12.0, 8.0, 12.0);

        assert_eq!(layout.column_count, 1);
    }

    #[test]
    fn long_description_layout_uses_three_columns_at_current_width() {
        let current = calculate_overlay_layout(1_715.0, 6, 170.0, 320.0, 12.0, 0.0, 12.0);
        let minimum = calculate_overlay_layout(880.0, 6, 170.0, 320.0, 12.0, 0.0, 12.0);

        assert_eq!(current.target_combo_width, 170.0);
        assert_eq!(current.target_action_width, 320.0);
        assert_eq!(current.column_count, 3);
        assert_eq!(minimum.column_count, 1);
    }

    #[test]
    fn short_wide_layout_uses_four_columns() {
        let layout = calculate_overlay_layout(1_715.0, 6, 112.0, 180.0, 12.0, 8.0, 12.0);

        assert_eq!(layout.column_count, 4);
    }

    #[test]
    fn adding_column_preserves_target_action_width() {
        let layout = calculate_overlay_layout(2_044.0, 6, 170.0, 320.0, 12.0, 0.0, 12.0);

        assert_eq!(layout.column_count, 4);
        assert_eq!(layout.action_width, layout.target_action_width);
    }

    #[test]
    fn column_transition_keeps_target_action_width() {
        let before = calculate_overlay_layout(1_529.5, 6, 170.0, 320.0, 12.0, 0.0, 12.0);
        let at_transition = calculate_overlay_layout(1_530.0, 6, 170.0, 320.0, 12.0, 0.0, 12.0);

        assert_eq!(before.column_count, 2);
        assert_eq!(at_transition.column_count, 3);
        assert!(before.action_width >= before.target_action_width);
        assert!(at_transition.action_width >= at_transition.target_action_width);
    }

    #[test]
    fn column_count_is_monotonic_across_resize_sequence() {
        let widths = [880.0, 1_016.0, 1_529.0, 1_530.0, 2_043.0, 2_044.0];
        let counts = widths.map(|width| {
            calculate_overlay_layout(width, 6, 170.0, 320.0, 12.0, 0.0, 12.0).column_count
        });

        assert_eq!(counts, [1, 2, 2, 3, 3, 4]);
        assert!(counts.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn maximum_style_values_reduce_columns_before_action_width_turns_negative() {
        let layout = calculate_overlay_layout(1_715.0, 6, 220.0, 320.0, 12.0, 32.0, 12.0);
        let cramped = calculate_overlay_layout(200.0, 6, 220.0, 320.0, 12.0, 32.0, 12.0);

        assert_eq!(layout.column_count, 2);
        assert!(layout.action_width >= layout.target_action_width);
        assert_eq!(cramped.column_count, 1);
        assert_eq!(cramped.action_width, 0.0);
    }

    #[test]
    fn shortcut_card_palette_applies_overlay_opacity() {
        let settings = OverlayStyleSettings::default();
        let low = shortcut_card_palette(&settings, 0.55);
        let high = shortcut_card_palette(&settings, 1.0);

        assert!(low.fill.a() < high.fill.a());
    }

    #[test]
    fn default_overlay_style_matches_current_visual_constants() {
        let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);

        assert_eq!(style.title_size, 18.0);
        assert_eq!(style.subtitle_size, 12.0);
        assert_eq!(style.group_heading_size, 13.0);
        assert_eq!(style.action_text_size, 12.0);
        assert_eq!(style.action_text_y_offset, -0.75);
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
    fn custom_action_text_y_offset_carries_to_resolved_style() {
        let settings = OverlayStyleSettings {
            action_text_y_offset: 2.25,
            ..Default::default()
        };

        assert_eq!(
            resolved_overlay_style(&settings, 0.96).action_text_y_offset,
            2.25
        );
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

        assert_eq!(
            palette.fill,
            egui::Color32::from_rgba_unmultiplied(252, 251, 247, 242)
        );
        assert_eq!(
            palette.border,
            egui::Color32::from_rgba_unmultiplied(210, 210, 205, 170)
        );
        assert_eq!(
            palette.heading,
            egui::Color32::from_rgba_unmultiplied(42, 42, 38, 255)
        );
        assert_eq!(
            palette.group_heading,
            egui::Color32::from_rgba_unmultiplied(34, 34, 31, 255)
        );
        assert_eq!(
            palette.text,
            egui::Color32::from_rgba_unmultiplied(52, 52, 48, 255)
        );
        assert_eq!(
            palette.weak_text,
            egui::Color32::from_rgba_unmultiplied(118, 116, 108, 255)
        );
        assert_eq!(
            palette.divider,
            egui::Color32::from_rgba_unmultiplied(202, 202, 196, 140)
        );
        assert_eq!(
            palette.keycap_fill,
            egui::Color32::from_rgba_unmultiplied(246, 245, 241, 230)
        );
        assert_eq!(
            palette.keycap_border,
            egui::Color32::from_rgba_unmultiplied(170, 170, 164, 170)
        );
        assert_eq!(
            palette.keycap_text,
            egui::Color32::from_rgba_unmultiplied(44, 44, 40, 255)
        );
    }

    #[test]
    fn resize_grip_toggle_controls_resize_interaction() {
        assert!(should_show_resize_grip(resolved_overlay_style(
            &OverlayStyleSettings::default(),
            0.96,
        )));
        let settings = OverlayStyleSettings {
            show_resize_grip: false,
            ..Default::default()
        };

        assert!(!should_show_resize_grip(resolved_overlay_style(
            &settings, 0.96,
        )));
    }

    #[test]
    fn divider_toggle_controls_column_dividers() {
        assert!(should_show_column_dividers(resolved_overlay_style(
            &OverlayStyleSettings::default(),
            0.96,
        )));
        let settings = OverlayStyleSettings {
            show_column_dividers: false,
            ..Default::default()
        };

        assert!(!should_show_column_dividers(resolved_overlay_style(
            &settings, 0.96,
        )));
    }

    #[test]
    fn empty_message_toggle_controls_empty_state() {
        assert!(should_show_empty_message(resolved_overlay_style(
            &OverlayStyleSettings::default(),
            0.96,
        )));
        let settings = OverlayStyleSettings {
            show_empty_message: false,
            ..Default::default()
        };

        assert!(!should_show_empty_message(resolved_overlay_style(
            &settings, 0.96,
        )));
    }

    #[test]
    fn long_keycap_combos_keep_right_edge_aligned() {
        with_font_measurement_ui(|ui| {
            let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);
            let parts = combo_keycap_parts("Ctrl+Shift+Space");
            let run_width = measure_combo_run_width(ui, &parts, style);
            let start = keycap_combo_start_x(0.0, style.combo_width, run_width);

            assert!((start + run_width - style.combo_width).abs() < 0.01);
        });
    }

    #[test]
    #[allow(deprecated)]
    fn oversized_combo_is_clipped_from_the_left_and_keeps_right_edge() {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        let style = resolved_overlay_style(
            &OverlayStyleSettings {
                combo_width: 220.0,
                ..Default::default()
            },
            0.96,
        );
        let combo = "Ctrl+Shift+Alt+Super+Command+Option+Delete+Enter";
        let combo_rect =
            egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(220.0, style.row_height));
        let mut overflowed = None;
        let mut run_width = None;

        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let parts = combo_keycap_parts(combo);
                let measured_width = measure_combo_run_width(ui, &parts, style);
                run_width = Some(measured_width);
                overflowed = Some(show_keycap_combo(ui, combo_rect, combo, style));
            });
        });

        let run_width = run_width.expect("combo should be measured");
        assert!(run_width > combo_rect.width());
        assert_eq!(overflowed, Some(true));
        assert!(
            keycap_combo_start_x(combo_rect.left(), combo_rect.right(), run_width)
                < combo_rect.left()
        );

        let keycap_shapes = output.shapes.iter().filter(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape) => {
                shape.fill == style.palette.keycap_fill
                    || shape.stroke.color == style.palette.keycap_border
            }
            egui::Shape::Text(shape) => shape.fallback_color == style.palette.keycap_text,
            _ => false,
        });
        let keycap_shapes = keycap_shapes.collect::<Vec<_>>();
        assert!(!keycap_shapes.is_empty());
        assert!(
            keycap_shapes
                .iter()
                .all(|shape| shape.clip_rect == combo_rect)
        );

        let rightmost_badge_edge = keycap_shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == style.palette.keycap_fill => {
                    Some(shape.rect.right())
                }
                _ => None,
            })
            .max_by(f32::total_cmp)
            .expect("at least one keycap fill should be painted");
        assert!((rightmost_badge_edge - combo_rect.right()).abs() < 0.01);
    }

    #[test]
    fn clipped_combo_requests_full_text_tooltip() {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        ctx.global_style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let style = resolved_overlay_style(
            &OverlayStyleSettings {
                combo_width: 220.0,
                ..Default::default()
            },
            0.96,
        );
        let combo = "Ctrl+Shift+Alt+Super+Command+Option+Delete+Enter";
        let entry = ShortcutEntry {
            combo: combo.to_owned(),
            action: "Command palette".to_owned(),
            group: "Commands".to_owned(),
            source: crate::core::ShortcutSource::User,
        };

        let first_frame = render_shortcut_columns_frame(&ctx, &entry, style, None, 0.0);
        let combo_rect = first_frame
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == style.palette.keycap_fill => {
                    Some(clipped.clip_rect)
                }
                _ => None,
            })
            .expect("production renderer should paint the combo rail");
        let _ = render_shortcut_columns_frame(&ctx, &entry, style, Some(combo_rect.center()), 1.0);
        let hovered_frame = render_shortcut_columns_frame(&ctx, &entry, style, None, 2.0);

        assert!(hovered_frame.shapes.iter().any(|clipped| {
            matches!(
                &clipped.shape,
                egui::Shape::Text(shape) if shape.galley.job.text == combo
            )
        }));
    }

    #[test]
    fn short_combo_does_not_request_full_text_tooltip() {
        let ctx = egui::Context::default();
        install_korean_font(&ctx).expect("production Korean font installation should succeed");
        ctx.global_style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let style = resolved_overlay_style(
            &OverlayStyleSettings {
                combo_width: 220.0,
                ..Default::default()
            },
            0.96,
        );
        let combo = "Ctrl+P";
        let entry = ShortcutEntry {
            combo: combo.to_owned(),
            action: "Command palette".to_owned(),
            group: "Commands".to_owned(),
            source: crate::core::ShortcutSource::User,
        };

        let first_frame = render_shortcut_columns_frame(&ctx, &entry, style, None, 0.0);
        let combo_rect = first_frame
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == style.palette.keycap_fill => {
                    Some(clipped.clip_rect)
                }
                _ => None,
            })
            .expect("production renderer should paint the combo rail");
        let _ = render_shortcut_columns_frame(&ctx, &entry, style, Some(combo_rect.center()), 1.0);
        let hovered_frame = render_shortcut_columns_frame(&ctx, &entry, style, None, 2.0);

        assert!(!hovered_frame.shapes.iter().any(|clipped| {
            matches!(
                &clipped.shape,
                egui::Shape::Text(shape) if shape.galley.job.text == combo
            )
        }));
        assert_eq!(overflow_tooltip_text(combo, false), None);
    }

    #[test]
    fn keycap_badge_has_enough_vertical_padding() {
        let style = resolved_overlay_style(&OverlayStyleSettings::default(), 0.96);

        assert_eq!(style.keycap_height, 16.0);
    }

    #[test]
    fn keycap_text_is_optically_centered_upward() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(32.0, 16.0));

        assert!(keycap_text_position(rect).y < rect.center().y);
    }

    #[test]
    fn action_text_position_changes_by_configured_offset() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(120.0, 18.0));

        assert_eq!(
            shortcut_action_text_position(rect, 2.25).y,
            rect.center().y + 2.25
        );
    }

    #[test]
    fn changing_action_text_y_offset_does_not_change_keycap_text_position() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(32.0, 16.0));
        let keycap_position = keycap_text_position(rect);

        let _action_position = shortcut_action_text_position(rect, 2.25);

        assert_eq!(keycap_text_position(rect), keycap_position);
    }

    #[test]
    fn default_overlay_style_settings_action_text_y_offset_is_visual_default() {
        assert_eq!(OverlayStyleSettings::default().action_text_y_offset, -0.75);
    }

    #[test]
    fn resize_grip_sits_in_bottom_right_corner() {
        let card = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 80.0));
        let grip = resize_grip_rect(card);

        assert_eq!(grip.right(), card.right() - RESIZE_GRIP_INSET);
        assert_eq!(grip.bottom(), card.bottom() - RESIZE_GRIP_INSET);
        assert_eq!(grip.width(), RESIZE_GRIP_SIZE);
        assert_eq!(grip.height(), RESIZE_GRIP_SIZE);
    }

    #[test]
    fn resize_delta_clamps_to_minimum_window_size() {
        let resized = resized_overlay_size(egui::vec2(930.0, 570.0), egui::vec2(-100.0, -100.0));

        assert_eq!(resized.x, WindowPlacement::MIN_WIDTH);
        assert_eq!(resized.y, WindowPlacement::MIN_HEIGHT);
    }

    #[test]
    fn shortcut_card_senses_drag_for_window_movement() {
        let sense = shortcut_card_drag_sense();

        assert!(sense.senses_drag());
    }

    #[test]
    fn shortcuts_view_has_no_transparent_outer_border() {
        assert_eq!(view_content_inset(AppView::Shortcuts), 0.0);
        assert_eq!(shortcut_card_outer_inset(AppView::Shortcuts), 0.0);
    }

    #[test]
    fn root_viewport_chrome_stays_overlay_only() {
        assert!(!viewport_decorations_for_view(AppView::Shortcuts));
        assert!(!viewport_resizable_for_view(AppView::Shortcuts));
    }

    #[test]
    fn settings_popup_viewport_uses_normal_window_chrome() {
        let viewport = settings_popup_viewport();

        assert_eq!(viewport.decorations, Some(true));
        assert_eq!(viewport.resizable, Some(true));
        assert_eq!(viewport.transparent, Some(false));
        assert_eq!(viewport.window_level, Some(egui::WindowLevel::AlwaysOnTop));
    }

    #[test]
    fn settings_popup_default_size_supports_two_column_card_layout() {
        let viewport = settings_popup_viewport();

        assert_eq!(viewport.inner_size, Some(egui::vec2(1080.0, 680.0)));
        assert_eq!(
            settings_card_column_count(settings_content_width(1080.0)),
            2
        );
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

    #[test]
    fn settings_card_renders_contents_without_mutating_state() {
        let palette = palette_for(egui::Theme::Dark);
        let mut called = false;

        egui::__run_test_ui(|ui| {
            settings_card(ui, "테스트", palette, |_ui| {
                called = true;
            });
        });

        assert!(called);
    }

    #[test]
    fn settings_card_can_fill_requested_vertical_space() {
        let palette = palette_for(egui::Theme::Dark);

        egui::__run_test_ui(|ui| {
            let response = settings_card_with_min_height(ui, "테스트", palette, 320.0, |_ui| {});

            assert!(response.rect.height() >= 320.0);
        });
    }

    #[test]
    fn compact_setting_rows_report_unchanged_without_input() {
        let mut number = 12.0;
        let mut toggle = true;
        let mut color = RgbaColor::rgba(1, 2, 3, 4);

        egui::__run_test_ui(|ui| {
            assert!(!overlay_style_number_row(
                ui,
                "숫자",
                &mut number,
                0.0..=24.0,
                "px"
            ));
            assert!(!settings_toggle_row(ui, "토글", &mut toggle));
            assert!(!rgba_color_edit(ui, "색상", &mut color));
        });

        assert_eq!(number, 12.0);
        assert!(toggle);
        assert_eq!(color, RgbaColor::rgba(1, 2, 3, 4));
    }

    #[test]
    fn settings_popup_uses_stable_viewport_id() {
        assert_eq!(settings_popup_viewport_id(), settings_popup_viewport_id());
    }

    #[test]
    fn settings_status_text_is_visible_only_when_present() {
        assert_eq!(
            settings_status_text("설정을 저장했습니다"),
            Some("설정을 저장했습니다")
        );
        assert_eq!(settings_status_text(""), None);
    }

    #[test]
    fn overlay_clear_color_is_transparent() {
        assert_eq!(transparent_clear_color()[3], 0.0);
    }

    #[test]
    fn shortcut_overlay_visual_fills_are_transparent() {
        let mut visuals = egui::Theme::Dark.default_visuals();
        apply_palette_to_visuals(&mut visuals, palette_for(egui::Theme::Dark));

        apply_view_visuals(&mut visuals, AppView::Shortcuts);

        assert_eq!(visuals.panel_fill.a(), 0);
        assert_eq!(visuals.window_fill.a(), 0);
    }

    #[test]
    fn group_heading_uses_bold_korean_font_family() {
        let font_id = group_heading_font_id(13.0);

        assert_eq!(font_id.size, 13.0);
        assert_eq!(
            font_id.family,
            egui::FontFamily::Name(std::sync::Arc::from(KOREAN_BOLD_FONT_FAMILY))
        );
    }

    #[test]
    fn tray_menu_restart_id_maps_to_restart_action() {
        assert_eq!(
            tray_action_for_menu_id(TRAY_MENU_RESTART),
            Some(TrayMenuAction::Restart)
        );
    }
}
