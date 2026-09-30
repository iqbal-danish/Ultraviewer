use eframe::egui::{self, Key};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    File,
    Navigation,
    Editing,
}

impl ActionCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::File => "File & General",
            Self::Navigation => "Navigation & View",
            Self::Editing => "Editing & Formatting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    // File
    NewFile,
    OpenFile,
    OpenUrl,
    Save,
    SaveAs,
    CloseFile,
    AboutDialog,
    KeyboardShortcuts,

    // Navigation & View
    CommandPalette,
    ToggleExplorer,
    ToggleStructure,
    ToggleAnalyzer,
    ToggleWordWrap,
    ToggleQueryBar,
    GotoLine,
    Find,
    FindNext,
    FindPrev,
    Replace,
    ToggleFilterView,
    ToggleHexView,
    NextTab,
    PrevTab,

    // Editing
    ToggleEditMode,
    Undo,
    Redo,
    FormatBeautify,
    FormatMinify,
    SelectAll,
    TransformUppercase,
    TransformLowercase,
}

impl ShortcutAction {
    pub fn all() -> &'static [ShortcutAction] {
        &[
            // File
            ShortcutAction::NewFile,
            ShortcutAction::OpenFile,
            ShortcutAction::OpenUrl,
            ShortcutAction::Save,
            ShortcutAction::SaveAs,
            ShortcutAction::CloseFile,
            ShortcutAction::KeyboardShortcuts,
            ShortcutAction::AboutDialog,

            // Navigation
            ShortcutAction::CommandPalette,
            ShortcutAction::Find,
            ShortcutAction::FindNext,
            ShortcutAction::FindPrev,
            ShortcutAction::Replace,
            ShortcutAction::GotoLine,
            ShortcutAction::ToggleQueryBar,
            ShortcutAction::ToggleExplorer,
            ShortcutAction::ToggleStructure,
            ShortcutAction::ToggleAnalyzer,
            ShortcutAction::ToggleWordWrap,
            ShortcutAction::ToggleFilterView,
            ShortcutAction::ToggleHexView,
            ShortcutAction::NextTab,
            ShortcutAction::PrevTab,

            // Editing
            ShortcutAction::ToggleEditMode,
            ShortcutAction::Undo,
            ShortcutAction::Redo,
            ShortcutAction::FormatBeautify,
            ShortcutAction::FormatMinify,
            ShortcutAction::SelectAll,
            ShortcutAction::TransformUppercase,
            ShortcutAction::TransformLowercase,
        ]
    }

    pub fn category(&self) -> ActionCategory {
        match self {
            Self::NewFile
            | Self::OpenFile
            | Self::OpenUrl
            | Self::Save
            | Self::SaveAs
            | Self::CloseFile
            | Self::KeyboardShortcuts
            | Self::AboutDialog => ActionCategory::File,

            Self::CommandPalette
            | Self::Find
            | Self::FindNext
            | Self::FindPrev
            | Self::Replace
            | Self::GotoLine
            | Self::ToggleQueryBar
            | Self::ToggleExplorer
            | Self::ToggleStructure
            | Self::ToggleAnalyzer
            | Self::ToggleWordWrap
            | Self::ToggleFilterView
            | Self::ToggleHexView
            | Self::NextTab
            | Self::PrevTab => ActionCategory::Navigation,

            Self::ToggleEditMode
            | Self::Undo
            | Self::Redo
            | Self::FormatBeautify
            | Self::FormatMinify
            | Self::SelectAll
            | Self::TransformUppercase
            | Self::TransformLowercase => ActionCategory::Editing,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::NewFile => "New Blank File",
            Self::OpenFile => "Open File",
            Self::OpenUrl => "Download / Open URL",
            Self::Save => "Save File",
            Self::SaveAs => "Save As...",
            Self::CloseFile => "Close Current File",
            Self::KeyboardShortcuts => "Keyboard Shortcuts Mapper",
            Self::AboutDialog => "About UltraViewer",

            Self::CommandPalette => "Command Palette",
            Self::Find => "Find in Document",
            Self::FindNext => "Find Next Match",
            Self::FindPrev => "Find Previous Match",
            Self::Replace => "Find & Replace",
            Self::GotoLine => "Go to Line",
            Self::ToggleQueryBar => "Toggle Query Bar (XPath / JSONPath)",
            Self::ToggleExplorer => "Toggle Folder Explorer",
            Self::ToggleStructure => "Toggle XML/JSON Structure Tree",
            Self::ToggleAnalyzer => "Toggle Field Analyzer",
            Self::ToggleWordWrap => "Toggle Word Wrap",
            Self::ToggleFilterView => "Toggle Filtered View (Grep Mode)",
            Self::ToggleHexView => "Toggle Hex & Binary Inspector",
            Self::NextTab => "Next Document Tab",
            Self::PrevTab => "Previous Document Tab",

            Self::ToggleEditMode => "Toggle In-Place Edit Mode",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::FormatBeautify => "Beautify Document",
            Self::FormatMinify => "Minify Document",
            Self::SelectAll => "Select All",
            Self::TransformUppercase => "Transform Selection to Uppercase",
            Self::TransformLowercase => "Transform Selection to Lowercase",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::NewFile => "Create a new empty scratch document",
            Self::OpenFile => "Select and load any multi-gigabyte or standard file",
            Self::OpenUrl => "Fetch and stream files from HTTP/HTTPS with resume support",
            Self::Save => "Save modified buffer back to the original file",
            Self::SaveAs => "Export modified document to a new file location",
            Self::CloseFile => "Close the active file and free memory",
            Self::KeyboardShortcuts => "Open this interactive keyboard mapper configuration",
            Self::AboutDialog => "Show application version, build info, and diagnostic stats",

            Self::CommandPalette => "Search and run any IDE action or preference",
            Self::Find => "Open search bar to locate text across large files",
            Self::FindNext => "Jump to the next search occurrence",
            Self::FindPrev => "Jump to the previous search occurrence",
            Self::Replace => "Open find and replace bar for in-place text replacement",
            Self::GotoLine => "Jump directly to any line number instantaneously",
            Self::ToggleQueryBar => "Open or close the XPath / JSONPath query bar",
            Self::ToggleExplorer => "Open or collapse directory explorer sidebar",
            Self::ToggleStructure => "Show hierarchical element/key structure tree",
            Self::ToggleAnalyzer => "Inspect schema statistics and field type distributions",
            Self::ToggleWordWrap => "Wrap lines at viewport width or maintain horizontal scrolling",
            Self::ToggleFilterView => "Isolate and display only lines matching the current search pattern",
            Self::ToggleHexView => "Toggle zero-copy hexadecimal and binary data inspection view",
            Self::NextTab => "Switch to the next opened document tab",
            Self::PrevTab => "Switch to the previous opened document tab",

            Self::ToggleEditMode => "Enable direct in-place line editing in the text view",
            Self::Undo => "Revert the last text modification",
            Self::Redo => "Re-apply the undone modification",
            Self::FormatBeautify => "Indent and format JSON/XML structure",
            Self::FormatMinify => "Strip whitespace to compress XML/JSON to compact form",
            Self::SelectAll => "Select entire line index range",
            Self::TransformUppercase => "Convert highlighted text to UPPERCASE",
            Self::TransformLowercase => "Convert highlighted text to lowercase",
        }
    }

    pub fn id_str(&self) -> &'static str {
        match self {
            Self::NewFile => "new_file",
            Self::OpenFile => "open_file",
            Self::OpenUrl => "open_url",
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::CloseFile => "close_file",
            Self::KeyboardShortcuts => "keyboard_shortcuts",
            Self::AboutDialog => "about",

            Self::CommandPalette => "command_palette",
            Self::Find => "find",
            Self::FindNext => "find_next",
            Self::FindPrev => "find_prev",
            Self::Replace => "replace",
            Self::GotoLine => "goto_line",
            Self::ToggleQueryBar => "toggle_query_bar",
            Self::ToggleExplorer => "toggle_explorer",
            Self::ToggleStructure => "toggle_structure",
            Self::ToggleAnalyzer => "toggle_analyzer",
            Self::ToggleWordWrap => "toggle_word_wrap",
            Self::ToggleFilterView => "toggle_filter_view",
            Self::ToggleHexView => "toggle_hex_view",
            Self::NextTab => "next_tab",
            Self::PrevTab => "prev_tab",

            Self::ToggleEditMode => "toggle_edit_mode",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::FormatBeautify => "format_beautify",
            Self::FormatMinify => "format_minify",
            Self::SelectAll => "select_all",
            Self::TransformUppercase => "transform_uppercase",
            Self::TransformLowercase => "transform_lowercase",
        }
    }

    pub fn from_id_str(s: &str) -> Option<Self> {
        match s {
            "new_file" => Some(Self::NewFile),
            "open_file" => Some(Self::OpenFile),
            "open_url" => Some(Self::OpenUrl),
            "save" => Some(Self::Save),
            "save_as" => Some(Self::SaveAs),
            "close_file" => Some(Self::CloseFile),
            "keyboard_shortcuts" => Some(Self::KeyboardShortcuts),
            "about" => Some(Self::AboutDialog),

            "command_palette" => Some(Self::CommandPalette),
            "find" => Some(Self::Find),
            "find_next" => Some(Self::FindNext),
            "find_prev" => Some(Self::FindPrev),
            "replace" => Some(Self::Replace),
            "goto_line" => Some(Self::GotoLine),
            "toggle_query_bar" => Some(Self::ToggleQueryBar),
            "toggle_explorer" => Some(Self::ToggleExplorer),
            "toggle_structure" => Some(Self::ToggleStructure),
            "toggle_analyzer" => Some(Self::ToggleAnalyzer),
            "toggle_word_wrap" => Some(Self::ToggleWordWrap),
            "toggle_filter_view" => Some(Self::ToggleFilterView),
            "toggle_hex_view" => Some(Self::ToggleHexView),
            "next_tab" => Some(Self::NextTab),
            "prev_tab" => Some(Self::PrevTab),

            "toggle_edit_mode" => Some(Self::ToggleEditMode),
            "undo" => Some(Self::Undo),
            "redo" => Some(Self::Redo),
            "format_beautify" => Some(Self::FormatBeautify),
            "format_minify" => Some(Self::FormatMinify),
            "select_all" => Some(Self::SelectAll),
            "transform_uppercase" => Some(Self::TransformUppercase),
            "transform_lowercase" => Some(Self::TransformLowercase),
            _ => None,
        }
    }

    pub fn default_chord(&self) -> Option<KeyChord> {
        match self {
            Self::NewFile => Some(KeyChord::ctrl(Key::N)),
            Self::OpenFile => Some(KeyChord::ctrl(Key::O)),
            Self::OpenUrl => Some(KeyChord::ctrl(Key::U)),
            Self::Save => Some(KeyChord::ctrl(Key::S)),
            Self::SaveAs => Some(KeyChord::ctrl_shift(Key::S)),
            Self::CloseFile => Some(KeyChord::ctrl(Key::W)),
            Self::KeyboardShortcuts => Some(KeyChord::ctrl(Key::K)),
            Self::AboutDialog => Some(KeyChord::plain(Key::F1)),

            Self::CommandPalette => Some(KeyChord::ctrl_shift(Key::P)),
            Self::Find => Some(KeyChord::ctrl(Key::F)),
            Self::FindNext => Some(KeyChord::plain(Key::F3)),
            Self::FindPrev => Some(KeyChord::shift(Key::F3)),
            Self::Replace => Some(KeyChord::ctrl(Key::H)),
            Self::GotoLine => Some(KeyChord::ctrl(Key::G)),
            Self::ToggleQueryBar => Some(KeyChord::ctrl_shift(Key::Q)),
            Self::ToggleExplorer => Some(KeyChord::ctrl(Key::B)),
            Self::ToggleStructure => Some(KeyChord::ctrl_shift(Key::T)),
            Self::ToggleAnalyzer => Some(KeyChord::ctrl_shift(Key::A)),
            Self::ToggleWordWrap => Some(KeyChord::alt(Key::Z)),
            Self::ToggleFilterView => Some(KeyChord::alt(Key::F)),
            Self::ToggleHexView => Some(KeyChord::ctrl_shift(Key::H)),
            Self::NextTab => Some(KeyChord::ctrl(Key::Tab)),
            Self::PrevTab => Some(KeyChord::ctrl_shift(Key::Tab)),

            Self::ToggleEditMode => Some(KeyChord::ctrl(Key::E)),
            Self::Undo => Some(KeyChord::ctrl(Key::Z)),
            Self::Redo => Some(KeyChord::ctrl(Key::Y)),
            Self::FormatBeautify => Some(KeyChord::ctrl_shift(Key::B)),
            Self::FormatMinify => Some(KeyChord::ctrl_shift(Key::M)),
            Self::SelectAll => Some(KeyChord::ctrl(Key::A)),
            Self::TransformUppercase => Some(KeyChord::ctrl_shift(Key::U)),
            Self::TransformLowercase => Some(KeyChord::ctrl_alt(Key::U)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl KeyChord {
    pub const fn new(ctrl: bool, shift: bool, alt: bool, key: Key) -> Self {
        Self { ctrl, shift, alt, key }
    }

    pub const fn plain(key: Key) -> Self {
        Self::new(false, false, false, key)
    }

    pub const fn ctrl(key: Key) -> Self {
        Self::new(true, false, false, key)
    }

    pub const fn alt(key: Key) -> Self {
        Self::new(false, false, true, key)
    }

    pub const fn shift(key: Key) -> Self {
        Self::new(false, true, false, key)
    }

    pub const fn ctrl_shift(key: Key) -> Self {
        Self::new(true, true, false, key)
    }

    pub const fn ctrl_alt(key: Key) -> Self {
        Self::new(true, false, true, key)
    }

    pub const fn alt_shift(key: Key) -> Self {
        Self::new(false, true, true, key)
    }

    pub fn matches(&self, input: &egui::InputState) -> bool {
        let is_ctrl = input.modifiers.command || input.modifiers.ctrl;
        let is_shift = input.modifiers.shift;
        let is_alt = input.modifiers.alt;

        is_ctrl == self.ctrl && is_shift == self.shift && is_alt == self.alt && input.key_pressed(self.key)
    }

    pub fn key_to_name(k: Key) -> &'static str {
        match k {
            Key::A => "A", Key::B => "B", Key::C => "C", Key::D => "D",
            Key::E => "E", Key::F => "F", Key::G => "G", Key::H => "H",
            Key::I => "I", Key::J => "J", Key::K => "K", Key::L => "L",
            Key::M => "M", Key::N => "N", Key::O => "O", Key::P => "P",
            Key::Q => "Q", Key::R => "R", Key::S => "S", Key::T => "T",
            Key::U => "U", Key::V => "V", Key::W => "W", Key::X => "X",
            Key::Y => "Y", Key::Z => "Z",
            Key::Num0 => "0", Key::Num1 => "1", Key::Num2 => "2", Key::Num3 => "3",
            Key::Num4 => "4", Key::Num5 => "5", Key::Num6 => "6", Key::Num7 => "7",
            Key::Num8 => "8", Key::Num9 => "9",
            Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3", Key::F4 => "F4",
            Key::F5 => "F5", Key::F6 => "F6", Key::F7 => "F7", Key::F8 => "F8",
            Key::F9 => "F9", Key::F10 => "F10", Key::F11 => "F11", Key::F12 => "F12",
            Key::ArrowUp => "Up", Key::ArrowDown => "Down",
            Key::ArrowLeft => "Left", Key::ArrowRight => "Right",
            Key::Enter => "Enter", Key::Escape => "Esc", Key::Tab => "Tab",
            Key::Space => "Space", Key::Backspace => "Backspace", Key::Delete => "Delete",
            Key::Home => "Home", Key::End => "End",
            Key::PageUp => "PageUp", Key::PageDown => "PageDown",
            Key::Comma => ",", Key::Period => ".", Key::Slash => "/",
            Key::Backslash => "\\", Key::Minus => "-", Key::Plus => "+",
            Key::Equals => "=", Key::Semicolon => ";",
            _ => "Key",
        }
    }

    pub fn name_to_key(s: &str) -> Option<Key> {
        let clean = s.trim().to_uppercase();
        match clean.as_str() {
            "A" => Some(Key::A), "B" => Some(Key::B), "C" => Some(Key::C), "D" => Some(Key::D),
            "E" => Some(Key::E), "F" => Some(Key::F), "G" => Some(Key::G), "H" => Some(Key::H),
            "I" => Some(Key::I), "J" => Some(Key::J), "K" => Some(Key::K), "L" => Some(Key::L),
            "M" => Some(Key::M), "N" => Some(Key::N), "O" => Some(Key::O), "P" => Some(Key::P),
            "Q" => Some(Key::Q), "R" => Some(Key::R), "S" => Some(Key::S), "T" => Some(Key::T),
            "U" => Some(Key::U), "V" => Some(Key::V), "W" => Some(Key::W), "X" => Some(Key::X),
            "Y" => Some(Key::Y), "Z" => Some(Key::Z),
            "0" | "NUM0" => Some(Key::Num0), "1" | "NUM1" => Some(Key::Num1),
            "2" | "NUM2" => Some(Key::Num2), "3" | "NUM3" => Some(Key::Num3),
            "4" | "NUM4" => Some(Key::Num4), "5" | "NUM5" => Some(Key::Num5),
            "6" | "NUM6" => Some(Key::Num6), "7" | "NUM7" => Some(Key::Num7),
            "8" | "NUM8" => Some(Key::Num8), "9" | "NUM9" => Some(Key::Num9),
            "F1" => Some(Key::F1), "F2" => Some(Key::F2), "F3" => Some(Key::F3), "F4" => Some(Key::F4),
            "F5" => Some(Key::F5), "F6" => Some(Key::F6), "F7" => Some(Key::F7), "F8" => Some(Key::F8),
            "F9" => Some(Key::F9), "F10" => Some(Key::F10), "F11" => Some(Key::F11), "F12" => Some(Key::F12),
            "UP" | "ARROWUP" => Some(Key::ArrowUp),
            "DOWN" | "ARROWDOWN" => Some(Key::ArrowDown),
            "LEFT" | "ARROWLEFT" => Some(Key::ArrowLeft),
            "RIGHT" | "ARROWRIGHT" => Some(Key::ArrowRight),
            "ENTER" | "RETURN" => Some(Key::Enter),
            "ESC" | "ESCAPE" => Some(Key::Escape),
            "TAB" => Some(Key::Tab),
            "SPACE" => Some(Key::Space),
            "BACKSPACE" => Some(Key::Backspace),
            "DELETE" | "DEL" => Some(Key::Delete),
            "HOME" => Some(Key::Home),
            "END" => Some(Key::End),
            "PAGEUP" | "PGUP" => Some(Key::PageUp),
            "PAGEDOWN" | "PGDN" => Some(Key::PageDown),
            "," | "COMMA" => Some(Key::Comma),
            "." | "PERIOD" => Some(Key::Period),
            "/" | "SLASH" => Some(Key::Slash),
            "\\" | "BACKSLASH" => Some(Key::Backslash),
            "-" | "MINUS" => Some(Key::Minus),
            "+" | "PLUS" => Some(Key::Plus),
            "=" | "EQUALS" => Some(Key::Equals),
            ";" | "SEMICOLON" => Some(Key::Semicolon),
            _ => None,
        }
    }

    pub fn to_chord_string(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(Self::key_to_name(self.key));
        parts.join("+")
    }

    pub fn from_str(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = None;

        for part in trimmed.split('+') {
            let part = part.trim();
            if part.eq_ignore_ascii_case("ctrl")
                || part.eq_ignore_ascii_case("control")
                || part.eq_ignore_ascii_case("cmd")
                || part.eq_ignore_ascii_case("command")
            {
                ctrl = true;
            } else if part.eq_ignore_ascii_case("shift") {
                shift = true;
            } else if part.eq_ignore_ascii_case("alt")
                || part.eq_ignore_ascii_case("opt")
                || part.eq_ignore_ascii_case("option")
            {
                alt = true;
            } else {
                key = Self::name_to_key(part);
            }
        }

        let key = key?;
        Some(Self { ctrl, shift, alt, key })
    }

    /// Try to capture a chord from current user input events (for interactive key recording).
    pub fn capture_from_input(input: &egui::InputState) -> Option<Self> {
        let ctrl = input.modifiers.command || input.modifiers.ctrl;
        let shift = input.modifiers.shift;
        let alt = input.modifiers.alt;

        // Check for non-modifier key press
        for event in &input.events {
            if let egui::Event::Key { key, pressed: true, .. } = event {
                // Ignore modifier-only key events
                let k = *key;
                if matches!(k, Key::Escape) && !ctrl && !shift && !alt {
                    // Esc alone is used to cancel recording
                    return None;
                }
                return Some(Self { ctrl, shift, alt, key: k });
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct KeybindingsConfig {
    pub bindings: HashMap<ShortcutAction, Option<KeyChord>>,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        for action in ShortcutAction::all() {
            bindings.insert(*action, action.default_chord());
        }

        Self {
            bindings,
        }
    }
}

impl KeybindingsConfig {
    pub fn config_path() -> Option<PathBuf> {
        let app_dir = if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("UltraViewer")
        } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            PathBuf::from(home).join(".ultraviewer")
        } else {
            return None;
        };
        let _ = fs::create_dir_all(&app_dir);
        Some(app_dir.join("keybindings.ini"))
    }

    pub fn load() -> Self {
        let mut cfg = Self::default();
        let path = match Self::config_path() {
            Some(p) => p,
            None => return cfg,
        };

        if !path.exists() {
            return cfg;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return cfg,
        };

        let mut section = "";
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }

            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if section == "bindings" {
                    if let Some(action) = ShortcutAction::from_id_str(k) {
                        if v.is_empty() || v.eq_ignore_ascii_case("none") {
                            cfg.bindings.insert(action, None);
                        } else if let Some(chord) = KeyChord::from_str(v) {
                            cfg.bindings.insert(action, Some(chord));
                        }
                    }
                }
            }
        }

        cfg
    }

    pub fn save(&self) {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return,
        };

        let mut out = String::new();
        out.push_str("# UltraViewer Keyboard Shortcuts Configuration\n\n");

        out.push_str("[bindings]\n");
        for action in ShortcutAction::all() {
            let chord_str = self.bindings.get(action).and_then(|c| *c).map(|c| c.to_chord_string()).unwrap_or_else(|| "none".to_string());
            out.push_str(&format!("{}={}\n", action.id_str(), chord_str));
        }

        let _ = fs::write(&path, out);
    }

    pub fn get(&self, action: ShortcutAction) -> Option<KeyChord> {
        self.bindings.get(&action).copied().flatten()
    }

    pub fn set(&mut self, action: ShortcutAction, chord: Option<KeyChord>) {
        self.bindings.insert(action, chord);
        self.save();
    }

    pub fn reset_action(&mut self, action: ShortcutAction) {
        self.bindings.insert(action, action.default_chord());
        self.save();
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
        self.save();
    }

    pub fn is_pressed(&self, action: ShortcutAction, input: &egui::InputState) -> bool {
        if let Some(chord) = self.get(action) {
            chord.matches(input)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_chord_string_roundtrip() {
        let chords = vec![
            KeyChord::ctrl(Key::U),
            KeyChord::ctrl_shift(Key::P),
            KeyChord::ctrl_alt(Key::U),
            KeyChord::plain(Key::F3),
            KeyChord::shift(Key::F3),
            KeyChord::alt(Key::Z),
        ];

        for chord in chords {
            let s = chord.to_chord_string();
            let parsed = KeyChord::from_str(&s).expect(&format!("Failed to parse chord string: {}", s));
            assert_eq!(chord, parsed, "Mismatch for string {}", s);
        }
    }

    #[test]
    fn test_key_defaults() {
        let cfg = KeybindingsConfig::default();
        assert_eq!(cfg.get(ShortcutAction::OpenUrl), Some(KeyChord::ctrl(Key::U)));
        assert_eq!(cfg.get(ShortcutAction::CommandPalette), Some(KeyChord::ctrl_shift(Key::P)));
        assert_eq!(cfg.get(ShortcutAction::KeyboardShortcuts), Some(KeyChord::ctrl(Key::K)));
    }

    #[test]
    fn test_action_id_roundtrip() {
        for action in ShortcutAction::all() {
            let id = action.id_str();
            let recovered = ShortcutAction::from_id_str(id);
            assert_eq!(Some(*action), recovered, "Failed for action {:?}", action);
        }
    }
}
