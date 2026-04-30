use cheatsheets::core::{ShortcutEntry, ShortcutSource, UserCatalog, UserShortcutPatch};
use cheatsheets::storage::{
    app_settings_path, load_app_user_catalog_from_customs_index,
    load_user_catalog_from_customs_dir, load_user_catalog_index_from_customs_dir,
    save_user_catalog_to_customs_dir, serialize_app_shortcuts_json, user_customs_dir,
};
use std::collections::BTreeMap;

#[test]
fn stores_runtime_files_under_home_config_cheatsheets() {
    let config_dir = dirs::home_dir()
        .unwrap()
        .join(".config")
        .join("CheatSheets");

    assert_eq!(user_customs_dir(), config_dir.join("Customs"));
    assert_eq!(app_settings_path(), config_dir.join("settings.json"));
}

#[test]
fn saves_each_app_to_its_own_custom_json_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut apps = BTreeMap::new();
    apps.insert(
        "wmux".to_owned(),
        vec![UserShortcutPatch::replace(ShortcutEntry::new(
            "Ctrl+B",
            "프리픽스 모드 토글",
            "전역",
            ShortcutSource::User,
        ))],
    );
    apps.insert(
        "code".to_owned(),
        vec![UserShortcutPatch::replace(ShortcutEntry::new(
            "Ctrl+P",
            "Open file by name",
            "Navigation",
            ShortcutSource::User,
        ))],
    );

    save_user_catalog_to_customs_dir(&UserCatalog { apps }, temp.path()).unwrap();

    assert!(temp.path().join("wmux.json").exists());
    assert!(temp.path().join("code.json").exists());
    let wmux_json = std::fs::read_to_string(temp.path().join("wmux.json")).unwrap();
    let wmux_value: serde_json::Value = serde_json::from_str(&wmux_json).unwrap();
    assert_eq!(wmux_value[0]["combo"], "Ctrl+B");
    assert!(wmux_value[0].get("kind").is_none());
    assert!(wmux_value[0].get("source").is_none());
}

#[test]
fn loads_all_json_files_from_customs_dir_using_file_stem_as_app_id() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("Wmux.json"),
        r#"[
  {
    "combo": "ctrl+b",
    "action": "프리픽스 모드 토글",
    "group": "전역"
  }
]"#,
    )
    .unwrap();
    std::fs::write(temp.path().join("notes.txt"), "ignored").unwrap();

    let catalog = load_user_catalog_from_customs_dir(temp.path()).unwrap();
    let patches = catalog.apps.get("wmux").unwrap();
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };

    assert_eq!(entry.combo, "Ctrl+B");
    assert_eq!(entry.action, "프리픽스 모드 토글");
    assert_eq!(entry.group, "전역");
    assert_eq!(entry.source, ShortcutSource::User);
}

#[test]
fn indexes_custom_json_files_without_reading_their_contents() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Code.json"), "not valid json").unwrap();
    std::fs::write(temp.path().join("notes.txt"), "ignored").unwrap();

    let index = load_user_catalog_index_from_customs_dir(temp.path()).unwrap();

    assert!(index.has_app("code"));
    assert!(!index.has_app("notes"));
}

#[test]
fn loads_only_requested_app_from_customs_index() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("Code.json"),
        r#"[{"combo":"ctrl+p","action":"Open file","group":"Navigation"}]"#,
    )
    .unwrap();
    std::fs::write(temp.path().join("Broken.json"), "not valid json").unwrap();

    let index = load_user_catalog_index_from_customs_dir(temp.path()).unwrap();
    let catalog = load_app_user_catalog_from_customs_index(&index, "code").unwrap();

    assert!(catalog.apps.contains_key("code"));
    assert!(!catalog.apps.contains_key("broken"));
}

#[test]
fn app_catalog_loader_reads_latest_file_contents_each_call() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Code.json");
    std::fs::write(
        &path,
        r#"[{"combo":"ctrl+p","action":"Open file","group":"Navigation"}]"#,
    )
    .unwrap();

    let index = load_user_catalog_index_from_customs_dir(temp.path()).unwrap();
    let first = load_app_user_catalog_from_customs_index(&index, "code").unwrap();
    std::fs::write(
        &path,
        r#"[{"combo":"ctrl+r","action":"Reload window","group":"Window"}]"#,
    )
    .unwrap();
    let second = load_app_user_catalog_from_customs_index(&index, "code").unwrap();

    let UserShortcutPatch::Replace { entry: first_entry } = &first.apps["code"][0] else {
        panic!("expected replace patch");
    };
    let UserShortcutPatch::Replace {
        entry: second_entry,
    } = &second.apps["code"][0]
    else {
        panic!("expected replace patch");
    };

    assert_eq!(first_entry.combo, "Ctrl+P");
    assert_eq!(second_entry.combo, "Ctrl+R");
}

#[test]
fn serializes_single_app_as_shortcut_array() {
    let patches = vec![UserShortcutPatch::replace(ShortcutEntry::new(
        "Ctrl+P",
        "Open file by name",
        "Navigation",
        ShortcutSource::User,
    ))];

    let json = serialize_app_shortcuts_json(&patches).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value[0]["combo"], "Ctrl+P");
    assert_eq!(value[0]["action"], "Open file by name");
    assert_eq!(value[0]["group"], "Navigation");
    assert!(value.get("apps").is_none());
}
