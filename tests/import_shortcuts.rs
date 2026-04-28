use cheatsheet::core::UserShortcutPatch;
use cheatsheet::import::{ImportFormat, parse_shortcut_import};

#[test]
fn imports_csv_rows_into_active_app_as_user_shortcuts() {
    let import = parse_shortcut_import(
        "combo,action,group\nCtrl+P,Open file by name,Navigation\nCtrl+Shift+F,Search in files,Search\n",
        ImportFormat::Csv,
        "Code",
    )
    .unwrap();

    let patches = import.catalog.apps.get("code").unwrap();

    assert_eq!(import.imported_count, 2);
    assert_eq!(patches.len(), 2);
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };
    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.action, "Open file by name");
    assert_eq!(entry.group, "Navigation");
}

#[test]
fn rejects_csv_rows_without_required_fields() {
    let error = parse_shortcut_import(
        "combo,action,group\nCtrl+P,,Navigation\n",
        ImportFormat::Csv,
        "code",
    )
    .unwrap_err();

    assert!(error.to_string().contains("row 1"));
}

#[test]
fn imports_json_user_catalog_and_counts_patches() {
    let import = parse_shortcut_import(
        r#"{
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
}"#,
        ImportFormat::Json,
        "ignored",
    )
    .unwrap();

    let patches = import.catalog.apps.get("code").unwrap();

    assert_eq!(import.imported_count, 1);
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };
    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.source, cheatsheet::core::ShortcutSource::User);
}

#[test]
fn imports_simplified_json_user_catalog() {
    let import = parse_shortcut_import(
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
        ImportFormat::Json,
        "ignored",
    )
    .unwrap();

    let patches = import.catalog.apps.get("code").unwrap();

    assert_eq!(import.imported_count, 1);
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };
    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.source, cheatsheet::core::ShortcutSource::User);
}

#[test]
fn imports_single_app_json_array_into_active_app() {
    let import = parse_shortcut_import(
        r#"[
  {
    "combo": "ctrl+p",
    "action": "Open file by name",
    "group": "Navigation"
  }
]"#,
        ImportFormat::Json,
        "Code",
    )
    .unwrap();

    let patches = import.catalog.apps.get("code").unwrap();

    assert_eq!(import.imported_count, 1);
    let UserShortcutPatch::Replace { entry } = &patches[0] else {
        panic!("expected replace patch");
    };
    assert_eq!(entry.combo, "Ctrl+P");
    assert_eq!(entry.action, "Open file by name");
}
