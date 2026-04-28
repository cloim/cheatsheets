#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::active_window;

#[cfg(not(target_os = "windows"))]
pub fn active_window() -> Option<AppIdentity> {
    use crate::core::AppIdentity;

    Some(AppIdentity::unknown())
}
