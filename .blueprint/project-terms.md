# Project Terms

## App Sheet

- Call each process-specific combo settings JSON file under `Customs` an `앱시트`.
- In English-facing code or documentation, use `app sheet`.
- Use the term for files such as `Customs\code.json`, `Customs\chrome.json`, and other per-process shortcut JSON files.
- An 앱시트 is a JSON array of shortcut entries with `combo`, `action`, and `group`.
- An 앱시트 can also be an object with `process_name`, `description`, `group_order`, and `shortcuts`.
- `process_name` is the display name used in the overlay title.
- `description` is the subtitle shown below the overlay title.
- `group_order` controls group display order by array element order.
- Groups missing from `group_order` appear after ordered groups in app-sheet registration order.
- Combos inside a group keep app-sheet registration order.
