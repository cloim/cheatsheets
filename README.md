# CheatSheets

A Windows shortcut overlay app. CheatSheets detects the active window process and shows built-in and custom shortcuts for that process.

[한국어 README](README.ko.md)

## Features

- Show or hide the overlay with `Ctrl+Shift+Space`
- Display shortcuts by active process
- Open settings or quit from the Windows tray menu
- Persist theme, opacity, overlay hotkey, and window placement settings
- Add or update custom shortcuts in the app
- Import shortcut files from JSON or CSV
- Index files in the `Customs` directory at startup, then lazily load each file only when its process overlay is needed

## Run

```powershell
cargo run
```

Build a release binary:

```powershell
cargo build --release
```

## Test

```powershell
cargo test
```

Check formatting:

```powershell
cargo fmt -- --check
```

## Release

GitHub Actions creates a Windows release when a version tag matching `v*` is pushed.

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds `target\release\cheatsheets.exe`, packages it with both README files, and uploads `CheatSheets-<tag>-windows-x64.zip` to the GitHub Release.

## Custom Shortcuts

Custom shortcuts are stored under:

```text
~\.config\CheatSheets\Customs
```

The file name becomes the process app id.

```text
Customs\code.json
Customs\chrome.json
```

Each file is an app sheet. The short form is a JSON array of shortcuts.

```json
[
  {
    "combo": "Ctrl+P",
    "action": "Open file by name",
    "group": "Navigation"
  }
]
```

If `group` is empty, CheatSheets uses the `Custom` group.

An app sheet can also use object form when it needs overlay metadata.

```json
{
  "process_name": "Process display name",
  "description": "Overlay subtitle",
  "group_order": ["Navigation", "Search"],
  "shortcuts": [
    {
      "combo": "Ctrl+P",
      "action": "Open file by name",
      "group": "Navigation"
    }
  ]
}
```

`process_name` is shown as the overlay title. `description` is shown as a subtitle below the title. `group_order` fixes group display order by array order. Groups not listed in `group_order` appear after ordered groups in their registration order. Combos inside each group also keep registration order.

## Import Formats

CSV imports shortcuts into the currently active process.

```csv
combo,action,group
Ctrl+P,Open file by name,Navigation
Ctrl+Shift+F,Search in files,Search
```

JSON imports can use either a single-app shortcut array or a multi-app catalog format.

## Development Notes

- Rust 2024 edition
- GUI: `eframe`/`egui`
- Global hotkeys: `global-hotkey`
- Tray icon: `tray-icon`
- Windows active-window detection: `windows` crate
