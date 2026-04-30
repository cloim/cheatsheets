use cheatsheets::core::{AppIdentity, Catalog, ShortcutEntry, ShortcutSource, UserShortcutPatch};

#[test]
fn app_identity_uses_lowercase_executable_stem_as_app_id() {
    let identity = AppIdentity::from_process_path(
        r"C:\Users\cloim\AppData\Local\Programs\Microsoft VS Code\Code.exe",
        "main.rs - cheatsheets",
    );

    assert_eq!(identity.app_id, "code");
    assert_eq!(identity.window_title, "main.rs - cheatsheets");
}

#[test]
fn user_shortcut_replaces_builtin_with_same_combo() {
    let mut catalog = Catalog::default();
    catalog.add_builtin(
        "code",
        ShortcutEntry::new(
            "Ctrl+P",
            "Quick Open",
            "Navigation",
            ShortcutSource::Builtin,
        ),
    );
    catalog.add_user_patch(
        "code",
        UserShortcutPatch::replace(ShortcutEntry::new(
            "ctrl+p",
            "Open file by name",
            "Navigation",
            ShortcutSource::User,
        )),
    );

    let sheet = catalog.sheet_for("code");

    assert_eq!(sheet.shortcuts.len(), 1);
    assert_eq!(sheet.shortcuts[0].combo, "Ctrl+P");
    assert_eq!(sheet.shortcuts[0].action, "Open file by name");
    assert_eq!(sheet.shortcuts[0].source, ShortcutSource::User);
}

#[test]
fn user_disabled_shortcut_hides_matching_builtin() {
    let mut catalog = Catalog::default();
    catalog.add_builtin(
        "code",
        ShortcutEntry::new(
            "Ctrl+Shift+P",
            "Command Palette",
            "Navigation",
            ShortcutSource::Builtin,
        ),
    );
    catalog.add_builtin(
        "code",
        ShortcutEntry::new(
            "Ctrl+P",
            "Quick Open",
            "Navigation",
            ShortcutSource::Builtin,
        ),
    );
    catalog.add_user_patch("code", UserShortcutPatch::disable("ctrl+shift+p"));

    let sheet = catalog.sheet_for("code");
    let combos: Vec<_> = sheet
        .shortcuts
        .iter()
        .map(|entry| entry.combo.as_str())
        .collect();

    assert_eq!(combos, vec!["Ctrl+P"]);
}

#[test]
fn user_shortcut_reload_replaces_previous_app_patches() {
    let mut catalog = Catalog::default();
    catalog.replace_user_patches(
        "code",
        vec![UserShortcutPatch::replace(ShortcutEntry::new(
            "Ctrl+P",
            "Open file",
            "Navigation",
            ShortcutSource::User,
        ))],
    );

    catalog.replace_user_patches(
        "code",
        vec![UserShortcutPatch::replace(ShortcutEntry::new(
            "Ctrl+R",
            "Reload window",
            "Window",
            ShortcutSource::User,
        ))],
    );

    let sheet = catalog.sheet_for("code");
    let combos: Vec<_> = sheet
        .shortcuts
        .iter()
        .map(|entry| entry.combo.as_str())
        .collect();

    assert_eq!(combos, vec!["Ctrl+R"]);
}
