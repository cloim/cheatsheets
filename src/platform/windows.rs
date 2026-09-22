use crate::core::AppIdentity;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::core::PWSTR;

pub fn active_window() -> Option<AppIdentity> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == HWND(std::ptr::null_mut::<c_void>()) {
        return None;
    }

    let title = window_title(hwnd);
    let pid = window_process_id(hwnd)?;
    let identity =
        AppIdentity::from_process_path(process_path_by_pid(pid).unwrap_or_default(), title);
    if is_windows_terminal_host(&identity)
        && let Some(processes) = process_snapshot()
        && let Some(child_identity) =
            resolve_terminal_child_identity(identity.clone(), pid, &processes)
    {
        return Some(child_identity);
    }

    Some(identity)
}

fn window_title(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

fn window_process_id(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 { None } else { Some(pid) }
}

fn process_path_by_pid(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(process) };

    result
        .ok()
        .map(|_| String::from_utf16_lossy(&buffer[..size as usize]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessSnapshotEntry {
    pid: u32,
    parent_pid: u32,
    process_path: String,
    created_at: u64,
}

fn process_snapshot() -> Option<Vec<ProcessSnapshotEntry>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();

    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let process_path =
                process_path_by_pid(entry.th32ProcessID).unwrap_or_else(|| exe_file_name(&entry));
            processes.push(ProcessSnapshotEntry {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
                process_path,
                created_at: process_created_at(entry.th32ProcessID),
            });

            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    let _ = unsafe { CloseHandle(snapshot) };
    Some(processes)
}

fn exe_file_name(entry: &PROCESSENTRY32W) -> String {
    let len = entry
        .szExeFile
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..len])
}

fn process_created_at(pid: u32) -> u64 {
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return 0;
    };
    let mut creation_time = FILETIME::default();
    let mut exit_time = FILETIME::default();
    let mut kernel_time = FILETIME::default();
    let mut user_time = FILETIME::default();
    let result = unsafe {
        GetProcessTimes(
            process,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        )
    };
    let _ = unsafe { CloseHandle(process) };

    if result.is_err() {
        return 0;
    }

    ((creation_time.dwHighDateTime as u64) << 32) | creation_time.dwLowDateTime as u64
}

fn is_windows_terminal_host(identity: &AppIdentity) -> bool {
    matches!(
        identity.app_id.as_str(),
        "windowsterminal" | "windowsterminalpreview"
    )
}

fn is_terminal_infrastructure_process(process_path: &str) -> bool {
    let identity = AppIdentity::from_process_path(process_path, "");
    matches!(
        identity.app_id.as_str(),
        "windowsterminal"
            | "windowsterminalpreview"
            | "openconsole"
            | "conhost"
            | "wt"
            | "pwsh"
            | "powershell"
            | "cmd"
    )
}

fn resolve_terminal_child_identity(
    host: AppIdentity,
    host_pid: u32,
    processes: &[ProcessSnapshotEntry],
) -> Option<AppIdentity> {
    let parent_by_pid: BTreeMap<_, _> = processes
        .iter()
        .map(|process| (process.pid, process.parent_pid))
        .collect();
    if let Some(profile_process) = matching_direct_profile_process(&host, host_pid, processes) {
        return Some(AppIdentity::from_process_path(
            &profile_process.process_path,
            host.window_title.clone(),
        ));
    }

    let mut descendants: Vec<_> = processes
        .iter()
        .filter(|process| !process.process_path.trim().is_empty())
        .filter(|process| !is_terminal_infrastructure_process(&process.process_path))
        .filter_map(|process| {
            descendant_depth(process.pid, host_pid, &parent_by_pid)
                .map(|depth| (depth, process.created_at, process.pid, process))
        })
        .collect();

    if let Some(title_process) = matching_descendant_title_process(&host, &descendants) {
        return Some(AppIdentity::from_process_path(
            &title_process.process_path,
            host.window_title,
        ));
    }

    descendants.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(right.2.cmp(&left.2))
    });

    descendants
        .iter()
        .find(|(_, _, _, process)| !is_terminal_helper_process(&process.process_path))
        .or_else(|| descendants.first())
        .map(|(_, _, _, process)| {
            AppIdentity::from_process_path(&process.process_path, host.window_title)
        })
}

fn matching_direct_profile_process<'a>(
    host: &AppIdentity,
    host_pid: u32,
    processes: &'a [ProcessSnapshotEntry],
) -> Option<&'a ProcessSnapshotEntry> {
    let active_tab_title = normalize_terminal_title(&host.window_title);
    if active_tab_title.is_empty() {
        return None;
    }

    let mut direct_matches: Vec<_> = processes
        .iter()
        .filter(|process| process.parent_pid == host_pid)
        .filter(|process| !process.process_path.trim().is_empty())
        .filter(|process| !is_terminal_infrastructure_process(&process.process_path))
        .filter(|process| process_matches_terminal_title(process, &active_tab_title))
        .collect();

    direct_matches.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then(right.pid.cmp(&left.pid))
    });
    direct_matches.first().copied()
}

fn process_matches_terminal_title(process: &ProcessSnapshotEntry, active_tab_title: &str) -> bool {
    let app_id = AppIdentity::from_process_path(&process.process_path, "").app_id;
    app_id == active_tab_title
        || active_tab_title
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part == app_id)
}

fn matching_descendant_title_process<'a>(
    host: &AppIdentity,
    descendants: &[(usize, u64, u32, &'a ProcessSnapshotEntry)],
) -> Option<&'a ProcessSnapshotEntry> {
    let active_tab_title = normalize_terminal_title(&host.window_title);
    if active_tab_title.is_empty() {
        return None;
    }

    let mut matches: Vec<_> = descendants
        .iter()
        .filter(|(_, _, _, process)| !is_terminal_helper_process(&process.process_path))
        .filter(|(_, _, _, process)| process_matches_terminal_title(process, &active_tab_title))
        .copied()
        .collect();

    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(right.2.cmp(&left.2))
    });
    matches.first().map(|(_, _, _, process)| *process)
}

fn is_terminal_helper_process(process_path: &str) -> bool {
    let identity = AppIdentity::from_process_path(process_path, "");
    matches!(
        identity.app_id.as_str(),
        "node" | "node_repl" | "mcp-server-windows-x64" | "inshellisense-win32-x64"
    )
}

fn normalize_terminal_title(title: &str) -> String {
    title.trim().to_ascii_lowercase()
}

fn descendant_depth(
    pid: u32,
    ancestor_pid: u32,
    parent_by_pid: &BTreeMap<u32, u32>,
) -> Option<usize> {
    let mut current_pid = pid;
    let mut depth = 0usize;
    let mut visited = BTreeSet::new();

    while visited.insert(current_pid) {
        let parent_pid = parent_by_pid.get(&current_pid).copied()?;
        if parent_pid == ancestor_pid {
            return Some(depth + 1);
        }
        if parent_pid == 0 {
            return None;
        }
        current_pid = parent_pid;
        depth += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_terminal_resolves_deepest_running_program_as_active_identity() {
        let host = AppIdentity::from_process_path(
            r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe",
            "codex",
        );
        let processes = vec![
            ProcessSnapshotEntry {
                pid: 10,
                parent_pid: 0,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"
                        .to_owned(),
                created_at: 1,
            },
            ProcessSnapshotEntry {
                pid: 11,
                parent_pid: 10,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\OpenConsole.exe"
                        .to_owned(),
                created_at: 2,
            },
            ProcessSnapshotEntry {
                pid: 12,
                parent_pid: 10,
                process_path: r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
                created_at: 3,
            },
            ProcessSnapshotEntry {
                pid: 13,
                parent_pid: 12,
                process_path: r"C:\Users\cloim\AppData\Local\Programs\Codex\codex.exe".to_owned(),
                created_at: 4,
            },
        ];

        let resolved = resolve_terminal_child_identity(host, 10, &processes).unwrap();

        assert_eq!(resolved.app_id, "codex");
        assert_eq!(
            resolved.process_path,
            r"C:\Users\cloim\AppData\Local\Programs\Codex\codex.exe"
        );
        assert_eq!(resolved.window_title, "codex");
    }

    #[test]
    fn windows_terminal_ignores_terminal_infrastructure_descendants() {
        let host = AppIdentity::from_process_path(
            r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe",
            "Windows Terminal",
        );
        let processes = vec![
            ProcessSnapshotEntry {
                pid: 10,
                parent_pid: 0,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"
                        .to_owned(),
                created_at: 1,
            },
            ProcessSnapshotEntry {
                pid: 11,
                parent_pid: 10,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\OpenConsole.exe"
                        .to_owned(),
                created_at: 2,
            },
        ];

        assert!(resolve_terminal_child_identity(host, 10, &processes).is_none());
    }

    #[test]
    fn windows_terminal_keeps_host_identity_when_only_shell_is_running() {
        let host = AppIdentity::from_process_path(
            r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe",
            "Downloads",
        );
        let processes = vec![
            ProcessSnapshotEntry {
                pid: 10,
                parent_pid: 0,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"
                        .to_owned(),
                created_at: 1,
            },
            ProcessSnapshotEntry {
                pid: 11,
                parent_pid: 10,
                process_path: r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
                created_at: 2,
            },
        ];

        assert!(resolve_terminal_child_identity(host, 10, &processes).is_none());
    }

    #[test]
    fn windows_terminal_prefers_profile_launcher_over_other_tab_descendants() {
        let host = AppIdentity::from_process_path(
            r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe",
            "psmux",
        );
        let processes = vec![
            ProcessSnapshotEntry {
                pid: 10,
                parent_pid: 0,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"
                        .to_owned(),
                created_at: 1,
            },
            ProcessSnapshotEntry {
                pid: 11,
                parent_pid: 10,
                process_path: r"C:\Users\cloim\AppData\Local\Microsoft\WinGet\Links\psmux.exe"
                    .to_owned(),
                created_at: 5,
            },
            ProcessSnapshotEntry {
                pid: 12,
                parent_pid: 10,
                process_path: r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
                created_at: 2,
            },
            ProcessSnapshotEntry {
                pid: 13,
                parent_pid: 12,
                process_path: r"C:\nvm4w\nodejs\node.exe".to_owned(),
                created_at: 3,
            },
            ProcessSnapshotEntry {
                pid: 14,
                parent_pid: 13,
                process_path: r"C:\nvm4w\nodejs\node.exe".to_owned(),
                created_at: 4,
            },
        ];

        let resolved = resolve_terminal_child_identity(host, 10, &processes).unwrap();

        assert_eq!(resolved.app_id, "psmux");
        assert_eq!(
            resolved.process_path,
            r"C:\Users\cloim\AppData\Local\Microsoft\WinGet\Links\psmux.exe"
        );
    }

    #[test]
    fn windows_terminal_prefers_title_matching_program_over_deeper_node_helper() {
        let host = AppIdentity::from_process_path(
            r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe",
            "codex",
        );
        let processes = vec![
            ProcessSnapshotEntry {
                pid: 10,
                parent_pid: 0,
                process_path:
                    r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"
                        .to_owned(),
                created_at: 1,
            },
            ProcessSnapshotEntry {
                pid: 11,
                parent_pid: 10,
                process_path: r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
                created_at: 2,
            },
            ProcessSnapshotEntry {
                pid: 12,
                parent_pid: 11,
                process_path: r"C:\nvm4w\nodejs\node.exe".to_owned(),
                created_at: 3,
            },
            ProcessSnapshotEntry {
                pid: 13,
                parent_pid: 12,
                process_path: r"C:\Users\cloim\AppData\Local\nvm\v24.15.0\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe"
                    .to_owned(),
                created_at: 4,
            },
            ProcessSnapshotEntry {
                pid: 14,
                parent_pid: 13,
                process_path: r"C:\nvm4w\nodejs\node.exe".to_owned(),
                created_at: 5,
            },
        ];

        let resolved = resolve_terminal_child_identity(host, 10, &processes).unwrap();

        assert_eq!(resolved.app_id, "codex");
        assert_eq!(
            resolved.process_path,
            r"C:\Users\cloim\AppData\Local\nvm\v24.15.0\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe"
        );
    }
}
