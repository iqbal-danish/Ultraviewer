use std::path::PathBuf;
use std::fs;

pub struct AppSession {
    pub open_files: Vec<PathBuf>,
    pub active_tab_idx: usize,
    pub recent_files: Vec<PathBuf>,
    pub font_size: f32,
    pub theme_name: String,
    pub word_wrap: bool,
}

impl Default for AppSession {
    fn default() -> Self {
        Self {
            open_files: Vec::new(),
            active_tab_idx: 0,
            recent_files: Vec::new(),
            font_size: 14.0,
            theme_name: "One Dark Pro Darker".to_string(),
            word_wrap: false,
        }
    }
}

impl AppSession {
    fn session_file_path() -> Option<PathBuf> {
        let app_dir = if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("UltraViewer")
        } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            PathBuf::from(home).join(".ultraviewer")
        } else {
            return None;
        };
        let _ = fs::create_dir_all(&app_dir);
        Some(app_dir.join("session.txt"))
    }

    pub fn load() -> Self {
        let path = match Self::session_file_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        let mut session = Self::default();
        let mut section = "";

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }

            match section {
                "open_files" => {
                    let p = PathBuf::from(line);
                    if p.exists() {
                        session.open_files.push(p);
                    }
                }
                "recent_files" => {
                    let p = PathBuf::from(line);
                    if p.exists() && !session.recent_files.contains(&p) {
                        session.recent_files.push(p);
                    }
                }
                "settings" => {
                    if let Some((k, v)) = line.split_once('=') {
                        let k = k.trim();
                        let v = v.trim();
                        match k {
                            "active_tab_idx" => {
                                if let Ok(idx) = v.parse::<usize>() {
                                    session.active_tab_idx = idx;
                                }
                            }
                            "font_size" => {
                                if let Ok(fs) = v.parse::<f32>() {
                                    session.font_size = fs.clamp(8.0, 36.0);
                                }
                            }
                            "theme" => {
                                session.theme_name = v.to_string();
                            }
                            "word_wrap" => {
                                session.word_wrap = v == "true" || v == "1";
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        session
    }

    pub fn save(&self) {
        let path = match Self::session_file_path() {
            Some(p) => p,
            None => return,
        };

        let mut out = String::new();
        out.push_str("[settings]\n");
        out.push_str(&format!("active_tab_idx={}\n", self.active_tab_idx));
        out.push_str(&format!("font_size={}\n", self.font_size));
        out.push_str(&format!("theme={}\n", self.theme_name));
        out.push_str(&format!("word_wrap={}\n", self.word_wrap));

        out.push_str("\n[open_files]\n");
        for p in &self.open_files {
            out.push_str(&p.to_string_lossy());
            out.push('\n');
        }

        out.push_str("\n[recent_files]\n");
        for p in self.recent_files.iter().take(15) {
            out.push_str(&p.to_string_lossy());
            out.push('\n');
        }

        let _ = fs::write(&path, out);
    }
}
