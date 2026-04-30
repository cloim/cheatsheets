use cheatsheets::core::{ShortcutEntry, ShortcutSource, UserCatalog, UserShortcutPatch};
use cheatsheets::storage::{
    parse_user_catalog_json, serialize_user_catalog_json, user_catalog_json_needs_migration,
};
use std::collections::BTreeMap;

#[test]
fn serializes_user_catalog_without_kind_or_source() {
    let mut apps = BTreeMap::new();
    apps.insert(
        "code".to_owned(),
        vec![UserShortcutPatch::replace(ShortcutEntry::new(
            "Ctrl+P",
            "Open file by name",
            "Navigation",
            ShortcutSource::User,
        ))],
    );

    let json = serialize_user_catalog_json(&UserCatalog { apps }).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["apps"]["code"][0]["combo"], "Ctrl+P");
    assert_eq!(value["apps"]["code"][0]["action"], "Open file by name");
    assert_eq!(value["apps"]["code"][0]["group"], "Navigation");
    assert!(value["apps"]["code"][0].get("kind").is_none());
    assert!(value["apps"]["code"][0].get("source").is_none());
}

#[test]
fn parses_simplified_user_catalog_as_user_replacements() {
    let catalog = parse_user_catalog_json(
        r#"{
  "apps": {
    "Code": [
      {
        "combo": "ctrl+p",
        "action": "Open file by name",
        "group": "Navigation"
      }
    ]
  }
}"#,
    )
    .unwrap();

    let patches = catalog.apps.get("code").unwrap();
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };

    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.action, "Open file by name");
    assert_eq!(entry.group, "Navigation");
    assert_eq!(entry.source, ShortcutSource::User);
}

#[test]
fn still_parses_legacy_kind_and_source_catalog() {
    let legacy_json = r#"{
  "apps": {
    "Code": [
      {
        "kind": "Replace",
        "entry": {
          "combo": "ctrl+p",
          "action": "Open file by name",
          "group": "Navigation",
          "source": "Builtin"
        }
      }
    ]
  }
}"#;
    let catalog = parse_user_catalog_json(legacy_json).unwrap();

    let patches = catalog.apps.get("code").unwrap();
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };

    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.source, ShortcutSource::User);
    assert!(user_catalog_json_needs_migration(legacy_json, &catalog).unwrap());
}

#[test]
fn simplified_catalog_does_not_need_migration_after_parse() {
    let simplified_json = r#"{
  "apps": {
    "code": [
      {
        "combo": "Ctrl+P",
        "action": "Open file by name",
        "group": "Navigation"
      }
    ]
  }
}"#;
    let catalog = parse_user_catalog_json(simplified_json).unwrap();

    assert!(!user_catalog_json_needs_migration(simplified_json, &catalog).unwrap());
}
