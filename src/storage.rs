use crate::core::{
    ShortcutEntry, ShortcutSource, UserCatalog, UserShortcutPatch, normalize_app_id,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn user_catalog_path() -> PathBuf {
    config_file_path("CheatSheet", "shortcuts.json")
}

pub fn user_customs_dir() -> PathBuf {
    config_dir("CheatSheet").join("Customs")
}

pub fn app_settings_path() -> PathBuf {
    config_file_path("CheatSheet", "settings.json")
}

fn config_file_path(app_dir: &str, file_name: &str) -> PathBuf {
    config_dir(app_dir).join(file_name)
}

fn config_dir(app_dir: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_dir)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Default,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub opacity: f32,
    pub toggle_hotkey: String,
    pub window: Option<WindowPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Default,
            opacity: 0.96,
            toggle_hotkey: "Ctrl+Shift+Space".to_owned(),
            window: None,
        }
    }
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.opacity = self.opacity.clamp(0.55, 1.0);
        if self.toggle_hotkey.trim().is_empty() {
            self.toggle_hotkey = "Ctrl+Shift+Space".to_owned();
        }
        if let Some(window) = &mut self.window {
            window.normalize();
        }
    }
}

impl WindowPlacement {
    pub const DEFAULT_WIDTH: f32 = 1080.0;
    pub const DEFAULT_HEIGHT: f32 = 680.0;
    pub const MIN_WIDTH: f32 = 920.0;
    pub const MIN_HEIGHT: f32 = 560.0;

    pub fn normalize(&mut self) {
        if !self.x.is_finite() {
            self.x = 0.0;
        }
        if !self.y.is_finite() {
            self.y = 0.0;
        }
        self.width = normalized_dimension(self.width, Self::MIN_WIDTH, Self::DEFAULT_WIDTH);
        self.height = normalized_dimension(self.height, Self::MIN_HEIGHT, Self::DEFAULT_HEIGHT);
    }

    pub fn differs_from(&self, other: &Self) -> bool {
        (self.x - other.x).abs() >= 1.0
            || (self.y - other.y).abs() >= 1.0
            || (self.width - other.width).abs() >= 1.0
            || (self.height - other.height).abs() >= 1.0
    }
}

fn normalized_dimension(value: f32, minimum: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.max(minimum)
    } else {
        default
    }
}

pub fn load_user_catalog() -> Result<UserCatalog> {
    load_user_catalog_from_customs_dir(&user_customs_dir())
}

pub fn load_user_catalog_index() -> Result<UserCatalogIndex> {
    load_user_catalog_index_from_customs_dir(&user_customs_dir())
}

pub fn save_user_catalog(catalog: &UserCatalog) -> Result<PathBuf> {
    let dir = user_customs_dir();
    save_user_catalog_to_customs_dir(catalog, &dir)?;
    Ok(dir)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserCatalogIndex {
    apps: BTreeMap<String, PathBuf>,
}

impl UserCatalogIndex {
    pub fn has_app(&self, app_id: &str) -> bool {
        self.apps.contains_key(&normalize_app_id(app_id))
    }

    fn path_for(&self, app_id: &str) -> Option<&Path> {
        self.apps
            .get(&normalize_app_id(app_id))
            .map(PathBuf::as_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredUserCatalog {
    apps: BTreeMap<String, Vec<StoredShortcutEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredShortcutEntry {
    combo: String,
    action: String,
    #[serde(default)]
    group: String,
}

pub fn parse_user_catalog_json(content: &str) -> Result<UserCatalog> {
    match serde_json::from_str::<StoredUserCatalog>(content) {
        Ok(stored) => stored_user_catalog_to_core(stored),
        Err(simplified_error) => {
            let legacy: UserCatalog = serde_json::from_str(content).with_context(|| {
                format!("단순 JSON 형식도 예전 JSON 형식도 아닙니다: {simplified_error}")
            })?;
            normalize_legacy_user_catalog(legacy)
        }
    }
}

pub fn load_user_catalog_from_customs_dir(dir: &Path) -> Result<UserCatalog> {
    let index = load_user_catalog_index_from_customs_dir(dir)?;
    let mut apps = BTreeMap::new();
    for app_id in index.apps.keys() {
        let catalog = load_app_user_catalog_from_customs_index(&index, app_id)?;
        apps.extend(catalog.apps);
    }

    Ok(UserCatalog { apps })
}

pub fn load_user_catalog_index_from_customs_dir(dir: &Path) -> Result<UserCatalogIndex> {
    if !dir.exists() {
        return Ok(UserCatalogIndex::default());
    }

    let mut apps = BTreeMap::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if !is_json_file(&path) {
            continue;
        }
        let Some(app_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let app_id = normalize_app_id(app_id);
        if app_id.is_empty() {
            continue;
        }

        apps.insert(app_id, path);
    }

    Ok(UserCatalogIndex { apps })
}

pub fn load_app_user_catalog_from_customs_index(
    index: &UserCatalogIndex,
    app_id: &str,
) -> Result<UserCatalog> {
    let app_id = normalize_app_id(app_id);
    let Some(path) = index.path_for(&app_id) else {
        return Ok(UserCatalog::default());
    };

    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let patches = parse_app_shortcuts_json(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    let mut apps = BTreeMap::new();
    apps.insert(app_id, patches);
    Ok(UserCatalog { apps })
}

pub fn save_user_catalog_to_customs_dir(catalog: &UserCatalog, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    for (app_id, patches) in &catalog.apps {
        let app_id = normalize_app_id(app_id);
        if app_id.is_empty() {
            continue;
        }

        let path = dir.join(format!("{app_id}.json"));
        let content = serialize_app_shortcuts_json(patches)?;
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

pub fn parse_app_shortcuts_json(content: &str) -> Result<Vec<UserShortcutPatch>> {
    let entries: Vec<StoredShortcutEntry> =
        serde_json::from_str(content).context("failed to parse shortcut array")?;
    entries
        .into_iter()
        .map(|entry| stored_entry_to_shortcut(entry, "custom file").map(UserShortcutPatch::replace))
        .collect()
}

pub fn serialize_app_shortcuts_json(patches: &[UserShortcutPatch]) -> Result<String> {
    let entries = patches
        .iter()
        .filter_map(|patch| match patch {
            UserShortcutPatch::Replace { entry } => Some(StoredShortcutEntry {
                combo: entry.combo.trim().to_owned(),
                action: entry.action.trim().to_owned(),
                group: normalized_group(&entry.group),
            }),
            UserShortcutPatch::Disable { .. } => None,
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&entries).context("failed to serialize shortcut array")
}

pub fn serialize_user_catalog_json(catalog: &UserCatalog) -> Result<String> {
    let mut apps: BTreeMap<String, Vec<StoredShortcutEntry>> = BTreeMap::new();
    for (app_id, patches) in &catalog.apps {
        let entries = patches
            .iter()
            .filter_map(|patch| match patch {
                UserShortcutPatch::Replace { entry } => Some(StoredShortcutEntry {
                    combo: entry.combo.trim().to_owned(),
                    action: entry.action.trim().to_owned(),
                    group: normalized_group(&entry.group),
                }),
                UserShortcutPatch::Disable { .. } => None,
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            apps.insert(normalize_app_id(app_id), entries);
        }
    }

    serde_json::to_string_pretty(&StoredUserCatalog { apps })
        .context("failed to serialize shortcuts")
}

pub fn user_catalog_json_needs_migration(content: &str, catalog: &UserCatalog) -> Result<bool> {
    let current: serde_json::Value = serde_json::from_str(content)?;
    let serialized = serialize_user_catalog_json(catalog)?;
    let migrated: serde_json::Value = serde_json::from_str(&serialized)?;
    Ok(current != migrated)
}

fn is_json_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn stored_user_catalog_to_core(stored: StoredUserCatalog) -> Result<UserCatalog> {
    let mut apps = BTreeMap::new();
    for (app_id, entries) in stored.apps {
        let normalized_app_id = normalize_app_id(app_id);
        let mut patches = Vec::with_capacity(entries.len());
        for entry in entries {
            patches.push(UserShortcutPatch::replace(stored_entry_to_shortcut(
                entry,
                &normalized_app_id,
            )?));
        }
        apps.insert(normalized_app_id, patches);
    }

    Ok(UserCatalog { apps })
}

fn stored_entry_to_shortcut(entry: StoredShortcutEntry, app_id: &str) -> Result<ShortcutEntry> {
    let combo = entry.combo.trim();
    let action = entry.action.trim();
    if combo.is_empty() || action.is_empty() {
        return Err(anyhow!("JSON app `{app_id}`: combo와 action은 필수입니다"));
    }

    Ok(ShortcutEntry::new(
        combo,
        action,
        normalized_group(&entry.group),
        ShortcutSource::User,
    ))
}

fn normalize_legacy_user_catalog(catalog: UserCatalog) -> Result<UserCatalog> {
    let mut apps = BTreeMap::new();

    for (app_id, patches) in catalog.apps {
        let normalized_app_id = normalize_app_id(app_id);
        let mut normalized_patches = Vec::new();
        for patch in patches {
            if let UserShortcutPatch::Replace { entry } = patch {
                let combo = entry.combo.trim();
                let action = entry.action.trim();
                if combo.is_empty() || action.is_empty() {
                    return Err(anyhow!(
                        "JSON app `{normalized_app_id}`: combo와 action은 필수입니다"
                    ));
                }
                normalized_patches.push(UserShortcutPatch::replace(ShortcutEntry::new(
                    combo,
                    action,
                    normalized_group(&entry.group),
                    ShortcutSource::User,
                )));
            }
        }
        if !normalized_patches.is_empty() {
            apps.insert(normalized_app_id, normalized_patches);
        }
    }

    Ok(UserCatalog { apps })
}

fn normalized_group(group: &str) -> String {
    let group = group.trim();
    if group.is_empty() {
        "Custom".to_owned()
    } else {
        group.to_owned()
    }
}

pub fn load_app_settings() -> Result<AppSettings> {
    let path = app_settings_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut settings: AppSettings = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    settings.normalize();
    Ok(settings)
}

pub fn save_app_settings(settings: &AppSettings) -> Result<PathBuf> {
    let path = app_settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut normalized = settings.clone();
    normalized.normalize();
    let content = serde_json::to_string_pretty(&normalized)?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}
