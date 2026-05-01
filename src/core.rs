use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIdentity {
    pub app_id: String,
    pub process_path: String,
    pub window_title: String,
}

impl AppIdentity {
    pub fn from_process_path(
        process_path: impl AsRef<str>,
        window_title: impl Into<String>,
    ) -> Self {
        let process_path = process_path.as_ref().to_owned();
        let app_id = Path::new(&process_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_ascii_lowercase();

        Self {
            app_id,
            process_path,
            window_title: window_title.into(),
        }
    }

    pub fn unknown() -> Self {
        Self {
            app_id: "unknown".to_owned(),
            process_path: String::new(),
            window_title: "Unknown active window".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShortcutSource {
    Builtin,
    User,
    Discovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutEntry {
    pub combo: String,
    pub action: String,
    pub group: String,
    pub source: ShortcutSource,
}

impl ShortcutEntry {
    pub fn new(
        combo: impl Into<String>,
        action: impl Into<String>,
        group: impl Into<String>,
        source: ShortcutSource,
    ) -> Self {
        Self {
            combo: normalize_combo_for_display(&combo.into()),
            action: action.into(),
            group: group.into(),
            source,
        }
    }

    pub fn key(&self) -> String {
        normalize_combo_key(&self.combo)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UserShortcutPatch {
    Replace { entry: ShortcutEntry },
    Disable { combo: String },
}

impl UserShortcutPatch {
    pub fn replace(entry: ShortcutEntry) -> Self {
        Self::Replace { entry }
    }

    pub fn disable(combo: impl Into<String>) -> Self {
        Self::Disable {
            combo: combo.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub builtin: BTreeMap<String, Vec<ShortcutEntry>>,
    pub user: BTreeMap<String, Vec<UserShortcutPatch>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppSheetConfig {
    pub process_name: Option<String>,
    pub description: Option<String>,
    pub group_order: Vec<String>,
}

impl Catalog {
    pub fn with_builtins() -> Self {
        let mut catalog = Self::default();
        catalog.seed_defaults();
        catalog
    }

    pub fn add_builtin(&mut self, app_id: impl Into<String>, entry: ShortcutEntry) {
        self.builtin
            .entry(normalize_app_id(app_id.into()))
            .or_default()
            .push(entry);
    }

    pub fn add_user_patch(&mut self, app_id: impl Into<String>, patch: UserShortcutPatch) {
        self.user
            .entry(normalize_app_id(app_id.into()))
            .or_default()
            .push(patch);
    }

    pub fn sheet_for(&self, app_id: &str) -> ShortcutSheet {
        self.sheet_for_with_config(app_id, &AppSheetConfig::default())
    }

    pub fn sheet_for_with_config(&self, app_id: &str, config: &AppSheetConfig) -> ShortcutSheet {
        let app_id = normalize_app_id(app_id);
        let display_name = config
            .process_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&app_id)
            .to_owned();
        let description = config
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty())
            .map(str::to_owned);
        let mut disabled = BTreeSet::new();
        let mut entries: BTreeMap<String, OrderedShortcutEntry> = BTreeMap::new();
        let mut next_position = 0;

        for entry in self.builtin.get(&app_id).into_iter().flatten() {
            entries.insert(
                entry.key(),
                OrderedShortcutEntry {
                    entry: entry.clone(),
                    position: next_position,
                },
            );
            next_position += 1;
        }

        for patch in self.user.get(&app_id).into_iter().flatten() {
            match patch {
                UserShortcutPatch::Replace { entry } => {
                    let key = entry.key();
                    disabled.remove(&key);
                    entries.insert(
                        key,
                        OrderedShortcutEntry {
                            entry: entry.clone(),
                            position: next_position,
                        },
                    );
                    next_position += 1;
                }
                UserShortcutPatch::Disable { combo } => {
                    let key = normalize_combo_key(combo);
                    disabled.insert(key.clone());
                    entries.remove(&key);
                }
            }
        }

        let mut shortcuts: Vec<_> = entries
            .into_iter()
            .filter_map(|(key, entry)| (!disabled.contains(&key)).then_some(entry))
            .collect();
        let group_order = group_order_positions(&config.group_order);
        let group_positions = group_first_positions(&shortcuts);
        shortcuts.sort_by(|a, b| {
            group_sort_key(&a.entry.group, &group_order, &group_positions)
                .cmp(&group_sort_key(
                    &b.entry.group,
                    &group_order,
                    &group_positions,
                ))
                .then(a.position.cmp(&b.position))
        });
        let shortcuts = shortcuts.into_iter().map(|entry| entry.entry).collect();

        ShortcutSheet {
            app_id,
            display_name,
            description,
            shortcuts,
        }
    }

    pub fn merge_user_catalog(&mut self, user_catalog: UserCatalog) {
        for (app_id, patches) in user_catalog.apps {
            self.user
                .entry(normalize_app_id(app_id))
                .or_default()
                .extend(patches);
        }
    }

    pub fn replace_user_patches(
        &mut self,
        app_id: impl Into<String>,
        patches: Vec<UserShortcutPatch>,
    ) {
        let app_id = normalize_app_id(app_id.into());
        if patches.is_empty() {
            self.user.remove(&app_id);
        } else {
            self.user.insert(app_id, patches);
        }
    }

    pub fn user_catalog(&self) -> UserCatalog {
        UserCatalog {
            apps: self.user.clone(),
        }
    }

    fn seed_defaults(&mut self) {
        let code = [
            ("Ctrl+P", "Quick Open", "Navigation"),
            ("Ctrl+Shift+P", "Command Palette", "Navigation"),
            ("Ctrl+B", "Toggle Sidebar", "Layout"),
            ("Ctrl+`", "Toggle Terminal", "Layout"),
            ("F5", "Start Debugging", "Debug"),
        ];
        let chrome = [
            ("Ctrl+L", "Focus Address Bar", "Navigation"),
            ("Ctrl+T", "New Tab", "Tabs"),
            ("Ctrl+Shift+T", "Reopen Closed Tab", "Tabs"),
            ("Ctrl+W", "Close Tab", "Tabs"),
            ("Ctrl+F", "Find in Page", "Page"),
        ];
        let explorer = [
            ("Ctrl+L", "Focus Address Bar", "Navigation"),
            ("Alt+Up", "Go Up", "Navigation"),
            ("Ctrl+Shift+N", "New Folder", "Files"),
            ("F2", "Rename", "Files"),
            ("Alt+Enter", "Properties", "Files"),
        ];

        for (combo, action, group) in code {
            self.add_builtin(
                "code",
                ShortcutEntry::new(combo, action, group, ShortcutSource::Builtin),
            );
        }
        for (combo, action, group) in chrome {
            self.add_builtin(
                "chrome",
                ShortcutEntry::new(combo, action, group, ShortcutSource::Builtin),
            );
        }
        for (combo, action, group) in explorer {
            self.add_builtin(
                "explorer",
                ShortcutEntry::new(combo, action, group, ShortcutSource::Builtin),
            );
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCatalog {
    pub apps: BTreeMap<String, Vec<UserShortcutPatch>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSheet {
    pub app_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub shortcuts: Vec<ShortcutEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderedShortcutEntry {
    entry: ShortcutEntry,
    position: usize,
}

fn group_order_positions(group_order: &[String]) -> BTreeMap<String, usize> {
    group_order
        .iter()
        .map(|group| normalized_group_name(group))
        .filter(|group| !group.is_empty())
        .enumerate()
        .map(|(index, group)| (group, index))
        .collect()
}

fn group_first_positions(entries: &[OrderedShortcutEntry]) -> BTreeMap<String, usize> {
    let mut positions = BTreeMap::new();
    for entry in entries {
        positions
            .entry(normalized_group_name(&entry.entry.group))
            .or_insert(entry.position);
    }
    positions
}

fn group_sort_key(
    group: &str,
    group_order: &BTreeMap<String, usize>,
    group_positions: &BTreeMap<String, usize>,
) -> (usize, usize) {
    let group = normalized_group_name(group);
    match group_order.get(&group) {
        Some(order) => (0, *order),
        None => (
            1,
            group_positions.get(&group).copied().unwrap_or(usize::MAX),
        ),
    }
}

fn normalized_group_name(group: &str) -> String {
    let group = group.trim();
    if group.is_empty() {
        "Custom".to_owned()
    } else {
        group.to_owned()
    }
}

pub fn normalize_app_id(app_id: impl AsRef<str>) -> String {
    app_id.as_ref().trim().to_ascii_lowercase()
}

pub fn normalize_combo_key(combo: &str) -> String {
    combo
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}

pub fn normalize_combo_for_display(combo: &str) -> String {
    combo
        .split('+')
        .map(|part| {
            let part = part.trim();
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "Ctrl".to_owned(),
                "shift" => "Shift".to_owned(),
                "alt" => "Alt".to_owned(),
                "win" | "super" | "meta" => "Win".to_owned(),
                key if key.len() == 1 => key.to_ascii_uppercase(),
                _ => part.to_owned(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}
