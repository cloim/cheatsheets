use cheatsheets::storage::{AppSettings, OverlayStyleSettings, RgbaColor, WindowPlacement};

#[test]
fn app_settings_loads_window_placement_from_json() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{
  "theme": "Default",
  "opacity": 0.96,
  "toggle_hotkey": "Ctrl+Shift+Space",
  "window": {
    "x": 120.0,
    "y": 80.0,
    "width": 1280.0,
    "height": 720.0
  }
}"#,
    )
    .unwrap();

    settings.normalize();

    assert_eq!(
        settings.window,
        Some(WindowPlacement {
            x: 120.0,
            y: 80.0,
            width: 1280.0,
            height: 720.0,
        })
    );
}

#[test]
fn app_settings_loads_default_overlay_style_when_missing_from_json() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{
  "theme": "Default",
  "opacity": 0.96,
  "toggle_hotkey": "Ctrl+Shift+Space",
  "window": null
}"#,
    )
    .unwrap();

    settings.normalize();

    assert_eq!(settings.overlay_style, Default::default());
}

#[test]
fn app_settings_clamps_window_placement_to_minimum_size() {
    let mut settings = AppSettings {
        window: Some(WindowPlacement {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        }),
        ..Default::default()
    };

    settings.normalize();

    let window = settings.window.unwrap();
    assert_eq!(window.width, 920.0);
    assert_eq!(window.height, 560.0);
}

#[test]
fn app_settings_serializes_window_placement() {
    let settings = AppSettings {
        window: Some(WindowPlacement {
            x: 120.0,
            y: 80.0,
            width: 1280.0,
            height: 720.0,
        }),
        ..Default::default()
    };

    let value: serde_json::Value = serde_json::to_value(settings).unwrap();

    assert_eq!(value["window"]["x"], 120.0);
    assert_eq!(value["window"]["y"], 80.0);
    assert_eq!(value["window"]["width"], 1280.0);
    assert_eq!(value["window"]["height"], 720.0);
}

#[test]
fn app_settings_serializes_overlay_style() {
    let settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            title_size: 22.0,
            card_background: RgbaColor::rgba(1, 2, 3, 200),
            show_column_dividers: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let value: serde_json::Value = serde_json::to_value(settings).unwrap();

    assert_eq!(value["overlay_style"]["title_size"], 22.0);
    assert_eq!(value["overlay_style"]["action_text_y_offset"], -0.75);
    assert_eq!(value["overlay_style"]["card_background"]["r"], 1);
    assert_eq!(value["overlay_style"]["card_background"]["a"], 200);
    assert_eq!(value["overlay_style"]["show_column_dividers"], false);
}

#[test]
fn overlay_style_defaults_action_text_y_offset() {
    assert_eq!(OverlayStyleSettings::default().action_text_y_offset, -0.75);
}

#[test]
fn overlay_style_normalize_clamps_action_text_y_offset() {
    let mut high_settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            action_text_y_offset: 99.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut low_settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            action_text_y_offset: -99.0,
            ..Default::default()
        },
        ..Default::default()
    };

    high_settings.normalize();
    low_settings.normalize();

    assert_eq!(high_settings.overlay_style.action_text_y_offset, 6.0);
    assert_eq!(low_settings.overlay_style.action_text_y_offset, -6.0);
}

#[test]
fn app_settings_defaults_missing_action_text_y_offset_from_json() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{
  "theme": "Default",
  "opacity": 0.96,
  "toggle_hotkey": "Ctrl+Shift+Space",
  "window": null,
  "overlay_style": {
    "title_size": 22.0
  }
}"#,
    )
    .unwrap();

    settings.normalize();

    assert_eq!(settings.overlay_style.action_text_y_offset, -0.75);
}

#[test]
fn overlay_style_normalize_clamps_font_and_layout_values() {
    let mut settings = AppSettings {
        overlay_style: OverlayStyleSettings {
            title_size: f32::NAN,
            subtitle_size: 99.0,
            group_heading_size: 1.0,
            action_text_size: 99.0,
            keycap_text_size: 1.0,
            card_padding: -1.0,
            row_height: 1.0,
            combo_width: 1.0,
            action_gap: 99.0,
            keycap_height: 28.0,
            keycap_gap: 99.0,
            card_radius: 99.0,
            ..Default::default()
        },
        ..Default::default()
    };

    settings.normalize();
    let style = settings.overlay_style;

    assert_eq!(style.title_size, OverlayStyleSettings::default().title_size);
    assert_eq!(style.subtitle_size, 24.0);
    assert_eq!(style.group_heading_size, 8.0);
    assert_eq!(style.action_text_size, 24.0);
    assert_eq!(style.keycap_text_size, 7.0);
    assert_eq!(style.card_padding, 0.0);
    assert_eq!(style.keycap_height, 28.0);
    assert_eq!(style.row_height, 28.0);
    assert_eq!(style.combo_width, 64.0);
    assert_eq!(style.action_gap, 32.0);
    assert_eq!(style.keycap_gap, 16.0);
    assert_eq!(style.card_radius, 24.0);
}
