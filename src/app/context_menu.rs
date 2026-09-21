use std::process::Command;

pub struct ContextMenuManager;

impl ContextMenuManager {
    /// Registers "Open with UltraViewer" in Windows Explorer context menu for all files and folders.
    /// Uses HKCU so it works without requiring Administrator elevation.
    pub fn register() -> Result<String, String> {
        #[cfg(target_os = "windows")]
        {
            let exe_path = std::env::current_exe()
                .map_err(|e| format!("Failed to get executable path: {}", e))?;
            let exe_str = exe_path.to_str()
                .ok_or_else(|| "Invalid executable path".to_string())?;

            // 1. Files: HKCU\Software\Classes\*\shell\UltraViewer
            let reg_file_key = r"HKCU\Software\Classes\*\shell\UltraViewer";
            let reg_file_cmd = format!(r#""{}" "%1""#, exe_str);

            let _ = Command::new("reg")
                .args(&["add", reg_file_key, "/ve", "/d", "Open with UltraViewer", "/f"])
                .output();
            let _ = Command::new("reg")
                .args(&["add", reg_file_key, "/v", "Icon", "/d", exe_str, "/f"])
                .output();
            let _ = Command::new("reg")
                .args(&["add", &format!(r"{}\command", reg_file_key), "/ve", "/d", &reg_file_cmd, "/f"])
                .output();

            // 2. Folders: HKCU\Software\Classes\Directory\shell\UltraViewer
            let reg_dir_key = r"HKCU\Software\Classes\Directory\shell\UltraViewer";
            let reg_dir_cmd = format!(r#""{}" "%1""#, exe_str);

            let _ = Command::new("reg")
                .args(&["add", reg_dir_key, "/ve", "/d", "Open Folder with UltraViewer", "/f"])
                .output();
            let _ = Command::new("reg")
                .args(&["add", reg_dir_key, "/v", "Icon", "/d", exe_str, "/f"])
                .output();
            let _ = Command::new("reg")
                .args(&["add", &format!(r"{}\command", reg_dir_key), "/ve", "/d", &reg_dir_cmd, "/f"])
                .output();

            Ok("Successfully added 'Open with UltraViewer' to Windows Explorer right-click context menu.".to_string())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("Context menu registration is only supported on Windows.".to_string())
        }
    }

    /// Removes "Open with UltraViewer" from Windows Explorer context menu.
    pub fn unregister() -> Result<String, String> {
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("reg")
                .args(&["delete", r"HKCU\Software\Classes\*\shell\UltraViewer", "/f"])
                .output();
            let _ = Command::new("reg")
                .args(&["delete", r"HKCU\Software\Classes\Directory\shell\UltraViewer", "/f"])
                .output();

            Ok("Successfully removed 'Open with UltraViewer' from Windows Explorer context menu.".to_string())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("Context menu unregistration is only supported on Windows.".to_string())
        }
    }
}
