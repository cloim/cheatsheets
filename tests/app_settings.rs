use cheatsheet::storage::{AppSettings, WindowPlacement};

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
