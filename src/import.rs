use crate::core::{
    ShortcutEntry, ShortcutSource, UserCatalog, UserShortcutPatch, normalize_app_id,
};
use crate::storage::{parse_app_shortcuts_json, parse_user_catalog_json};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Csv,
    Json,
}

impl ImportFormat {
    pub fn from_path(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("csv") => Ok(Self::Csv),
            Some("json") => Ok(Self::Json),
            _ => Err(anyhow!("지원하지 않는 파일 형식입니다: {}", path.display())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutImport {
    pub catalog: UserCatalog,
    pub imported_count: usize,
}

#[derive(Debug, Deserialize)]
struct CsvShortcutRow {
    #[serde(alias = "Combo")]
    combo: String,
    #[serde(alias = "Action")]
    action: String,
    #[serde(alias = "Group")]
    #[serde(default)]
    group: Option<String>,
}

pub fn parse_shortcut_import(
    content: &str,
    format: ImportFormat,
    active_app_id: &str,
) -> Result<ShortcutImport> {
    match format {
        ImportFormat::Csv => parse_csv_shortcuts(content, active_app_id),
        ImportFormat::Json => parse_json_shortcuts(content, active_app_id),
    }
}

fn parse_csv_shortcuts(content: &str, active_app_id: &str) -> Result<ShortcutImport> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());
    let mut patches = Vec::new();

    for (index, row) in reader.deserialize::<CsvShortcutRow>().enumerate() {
        let row = row.with_context(|| format!("CSV row {}를 읽지 못했습니다", index + 1))?;
        let combo = row.combo.trim();
        let action = row.action.trim();
        let group = row.group.as_deref().unwrap_or("Custom").trim();
        if combo.is_empty() || action.is_empty() {
            return Err(anyhow!(
                "CSV row {}: combo와 action은 필수입니다",
                index + 1
            ));
        }

        patches.push(UserShortcutPatch::replace(ShortcutEntry::new(
            combo,
            action,
            if group.is_empty() { "Custom" } else { group },
            ShortcutSource::User,
        )));
    }

    let imported_count = patches.len();
    let mut apps = BTreeMap::new();
    apps.insert(normalize_app_id(active_app_id), patches);
    Ok(ShortcutImport {
        catalog: UserCatalog { apps },
        imported_count,
    })
}

fn parse_json_shortcuts(content: &str, active_app_id: &str) -> Result<ShortcutImport> {
    let catalog = match parse_app_shortcuts_json(content) {
        Ok(patches) => {
            let mut apps = BTreeMap::new();
            apps.insert(normalize_app_id(active_app_id), patches);
            UserCatalog { apps }
        }
        Err(array_error) => parse_user_catalog_json(content)
            .with_context(|| format!("JSON 단축키 파일을 읽지 못했습니다: {array_error}"))?,
    };
    let imported_count = catalog.apps.values().map(Vec::len).sum();
    Ok(ShortcutImport {
        catalog,
        imported_count,
    })
}
