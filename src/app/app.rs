use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Color32, FontId, Key, RichText, ScrollArea, Sense, TextFormat, Ui, Vec2};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::editor::{EditorDocument, SaveManager, Viewport};
use crate::file_engine::{FileEngine, LineIndex};
use crate::formats::formatter::{FormatAction, FormattingProgress, JsonStreamingFormatter, XmlStreamingFormatter};
use crate::formats::json::{JsonStructureIndexer, JsonSyntaxHighlighter, JsonTreeNode, JsonValidationResult, JsonValidator};
use crate::formats::xml::{XmlStructureIndexer, XmlSyntaxHighlighter, XmlTreeNode, XmlValidationResult, XmlValidator};
use crate::analysis::field_analyzer::{AnalysisReport, StreamingFieldAnalyzer};
use crate::formats::{FileType, FormatDetector};
use crate::indexing::LineIndexer;
use crate::search::{SearchQuery, SearchResultMatch, SearchStatus, SearchWorker};
use super::activity_bar::{render_activity_bar, ActivityBarAction, ActivityBarProps, ActivityPanel};
use super::analyzer_panel::{render_analyzer_panel, AnalyzerPanelAction, AnalyzerPanelState};
use super::command_palette::{render_command_palette, CommandPaletteState, PaletteAction};
use super::context_menu::ContextMenuManager;
use super::csv_grid::{render_csv_grid, CsvGridState};
use super::diff_viewer::{render_diff_modal, DiffViewerAction, DiffViewerState};
use super::folder_explorer::{render_folder_explorer, FolderAction, FolderExplorerState};
use super::format_modal::{render_format_modal, FormatModalAction};
use super::json_tree_panel::render_json_tree_panel;
use super::menu::{render_menu_bar, MenuAction};
use super::overview_ruler::{render_overview_ruler, OverviewRulerProps};
use super::search_panel::{render_search_bar, render_search_results_panel, SearchBarAction};
use super::session::AppSession;
use super::status_bar::{format_number, render_status_bar, StatusBarAction, StatusBarProps};
use super::tab_bar::{render_tab_bar, TabBarAction, TabBarProps, TabInfo};
use super::theme::ColorTheme;
use super::xml_tree_panel::render_xml_tree_panel;

fn apply_modern_theme(ctx: &egui::Context, theme: ColorTheme) {
    theme.apply(ctx);
}

const VISIBLE_LINE_BUFFER: usize = 60;

pub struct ActiveAnalysis {
    pub cancel: Arc<AtomicBool>,
    pub rx: crossbeam_channel::Receiver<Result<AnalysisReport, String>>,
}

pub struct ActiveFormatting {
    pub title: String,
    pub action: FormatAction,
    pub src_path: PathBuf,
    pub dst_path: PathBuf,
    pub is_in_place: bool,
    pub progress: Arc<RwLock<FormattingProgress>>,
    pub cancel: Arc<AtomicBool>,
}

pub struct ActiveSaving {
    pub title: String,
    pub target_path: PathBuf,
    pub is_in_place: bool,
    pub progress: Arc<RwLock<FormattingProgress>>,
    pub cancel: Arc<AtomicBool>,
    pub rx: crossbeam_channel::Receiver<Result<PathBuf, String>>,
}

pub enum PendingFileAction {
    Open(PathBuf),
    Close,
    Exit,
}

/// State for the Full Line Inspector popup window.
/// Holds the full, un-truncated text of a single line fetched directly
/// from the file engine — nothing more is kept in memory.
pub struct FullLineInspector {
    pub line_number: usize,
    pub byte_len: usize,
    pub full_text: String,
}

pub struct OpenDocState {
    pub id: usize,
    pub path: PathBuf,
    pub engine: Arc<FileEngine>,
    pub line_index: Arc<LineIndex>,
    pub indexer_cancel: Arc<AtomicBool>,
    pub indexer_handle: Option<JoinHandle<()>>,
    pub viewport: Viewport,
    pub file_type: Option<FileType>,
    pub open_duration: Option<Duration>,
    pub current_line: usize,
    pub document: Option<EditorDocument>,
    pub is_edit_mode: bool,
    pub active_edit_line: Option<usize>,
    pub edit_line_buffer: String,
    pub xml_tree_root: Option<Arc<XmlTreeNode>>,
    pub json_tree_root: Option<Arc<JsonTreeNode>>,
}

pub struct UltraViewerApp {
    engine: Option<Arc<FileEngine>>,
    line_index: Option<Arc<LineIndex>>,
    indexer_cancel: Option<Arc<AtomicBool>>,
    indexer_handle: Option<JoinHandle<()>>,

    viewport: Viewport,
    file_type: Option<FileType>,
    open_duration: Option<Duration>,
    font_size: f32,

    current_line: usize,
    jump_line_input: String,
    jump_offset_input: String,

    // Search state
    show_search_bar: bool,
    show_search_results: bool,
    focus_search_input: bool,
    search_query: SearchQuery,
    last_executed_query: Option<SearchQuery>,
    auto_jump_to_first_match: bool,
    search_status: Arc<RwLock<SearchStatus>>,
    search_matches: Arc<RwLock<Vec<SearchResultMatch>>>,
    search_cancel: Option<Arc<AtomicBool>>,
    search_handle: Option<JoinHandle<()>>,
    active_match_idx: Option<usize>,

    // XML Intelligence state
    enable_syntax_highlighting: bool,
    word_wrap: bool,
    show_xml_tree: bool,
    xml_tree_root: Option<Arc<XmlTreeNode>>,
    xml_tree_building: bool,
    xml_tree_rx: Option<crossbeam_channel::Receiver<Option<XmlTreeNode>>>,
    xml_tree_cancel: Option<Arc<AtomicBool>>,
    xml_validation_result: Option<XmlValidationResult>,
    is_validating_xml: bool,

    // JSON Intelligence state
    show_json_tree: bool,
    json_tree_root: Option<Arc<JsonTreeNode>>,
    json_tree_building: bool,
    json_tree_rx: Option<crossbeam_channel::Receiver<Arc<JsonTreeNode>>>,
    json_tree_cancel: Option<Arc<AtomicBool>>,
    json_validation_result: Option<JsonValidationResult>,
    is_validating_json: bool,

    show_about_dialog: bool,
    show_goto_line_dialog: bool,
    active_formatting: Option<ActiveFormatting>,
    show_format_modal: bool,
    analyzer_state: AnalyzerPanelState,
    active_analysis: Option<ActiveAnalysis>,
    error_message: Option<String>,

    // Phase 8 Editing Engine State
    pub document: Option<EditorDocument>,
    pub is_edit_mode: bool,
    pub active_edit_line: Option<usize>,
    pub edit_line_buffer: String,
    pub active_saving: Option<ActiveSaving>,
    pub show_save_modal: bool,
    pub show_unsaved_dialog: bool,
    pub pending_file_action: Option<PendingFileAction>,

    // Full Line Inspector
    pub show_full_line_inspector: bool,
    pub full_line_inspector: Option<FullLineInspector>,

    pub active_activity_panel: ActivityPanel,
    pub sidebar_expanded: bool,
    pub titlebar_applied_frames: u8,
    pub recent_files: Vec<PathBuf>,

    // Multi-tab documents state
    pub tabs: Vec<OpenDocState>,
    pub active_tab_id: Option<usize>,
    pub next_tab_id: usize,

    // Power Pack modules state
    pub current_theme: ColorTheme,
    pub session: AppSession,
    pub command_palette: CommandPaletteState,
    pub folder_explorer: FolderExplorerState,
    pub csv_grid: CsvGridState,
    pub diff_viewer: DiffViewerState,
    pub status_notification: Option<(String, Instant)>,

    system_info: System,
    current_pid: Option<Pid>,
    last_sys_refresh: Instant,
    cached_rss_bytes: u64,
}

impl Default for UltraViewerApp {
    fn default() -> Self {
        let current_pid = sysinfo::get_current_pid().ok();
        let mut system_info = System::new();
        let mut cached_rss = 0;
        if let Some(pid) = current_pid {
            system_info.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
            if let Some(proc) = system_info.process(pid) {
                cached_rss = proc.memory();
            }
        }

        let session = AppSession::load();
        let current_theme = match session.theme_name.as_str() {
            "GitHub Dark" => ColorTheme::GitHubDark,
            "Monokai Pro" => ColorTheme::MonokaiPro,
            "Tokyo Night" => ColorTheme::TokyoNight,
            "VS Code Light Modern" | "Light Modern" => ColorTheme::LightModern,
            _ => ColorTheme::OneDarkProDarker,
        };
        let font_size = session.font_size;
        let word_wrap = session.word_wrap;
        let recent_files = session.recent_files.clone();

        Self {
            engine: None,
            line_index: None,
            indexer_cancel: None,
            indexer_handle: None,
            viewport: Viewport::new(),
            file_type: None,
            open_duration: None,
            font_size,
            current_line: 1,
            jump_line_input: "1".to_string(),
            jump_offset_input: "0".to_string(),

            show_search_bar: false,
            show_search_results: false,
            focus_search_input: false,
            search_query: SearchQuery::default(),
            last_executed_query: None,
            auto_jump_to_first_match: false,
            search_status: Arc::new(RwLock::new(SearchStatus::Idle)),
            search_matches: Arc::new(RwLock::new(Vec::new())),
            search_cancel: None,
            search_handle: None,
            active_match_idx: None,

            enable_syntax_highlighting: true,
            word_wrap,
            show_xml_tree: false,
            xml_tree_root: None,
            xml_tree_building: false,
            xml_tree_rx: None,
            xml_tree_cancel: None,
            xml_validation_result: None,
            is_validating_xml: false,

            show_json_tree: false,
            json_tree_root: None,
            json_tree_building: false,
            json_tree_rx: None,
            json_tree_cancel: None,
            json_validation_result: None,
            is_validating_json: false,

            show_about_dialog: false,
            show_goto_line_dialog: false,
            active_formatting: None,
            show_format_modal: false,
            analyzer_state: AnalyzerPanelState::default(),
            active_analysis: None,
            error_message: None,

            document: None,
            is_edit_mode: false,
            active_edit_line: None,
            edit_line_buffer: String::new(),
            active_saving: None,
            show_save_modal: false,
            show_unsaved_dialog: false,
            pending_file_action: None,

            show_full_line_inspector: false,
            full_line_inspector: None,

            active_activity_panel: ActivityPanel::None,
            sidebar_expanded: false,
            titlebar_applied_frames: 0,
            recent_files,

            tabs: Vec::new(),
            active_tab_id: None,
            next_tab_id: 1,

            current_theme,
            session,
            command_palette: CommandPaletteState::default(),
            folder_explorer: FolderExplorerState::default(),
            csv_grid: CsvGridState::default(),
            diff_viewer: DiffViewerState::default(),
            status_notification: None,

            system_info,
            current_pid,
            last_sys_refresh: Instant::now(),
            cached_rss_bytes: cached_rss,
        }
    }
}

impl UltraViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        apply_modern_theme(&cc.egui_ctx, app.current_theme);

        // Restore open files from session if any
        let to_open = app.session.open_files.clone();
        for p in to_open {
            if p.exists() {
                app.open_file(&p);
            }
        }
        app
    }

    /// Open a file from disk, initialize line index, and spawn background indexer.
    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) {
        let path_buf = path.as_ref().to_path_buf();
        if self.document.as_ref().map_or(false, |d| d.is_dirty()) {
            self.pending_file_action = Some(PendingFileAction::Open(path_buf));
            self.show_unsaved_dialog = true;
            return;
        }
        self.do_open_file(&path_buf);
    }

    pub fn do_open_file(&mut self, path: &Path) {
        let path_buf = path.to_path_buf();
        if let Some(existing) = self.tabs.iter().find(|t| t.path == path_buf) {
            let id = existing.id;
            self.switch_to_tab(id);
            return;
        }

        self.sync_active_tab_state();
        self.cancel_active_indexer();
        self.cancel_search();
        self.cancel_tree_builders();
        self.cancel_analysis();
        self.analyzer_state = AnalyzerPanelState::default();

        let start_time = Instant::now();
        match FileEngine::open(path) {
            Ok(raw_engine) => {
                if let Some(pos) = self.recent_files.iter().position(|p| p == &path_buf) {
                    self.recent_files.remove(pos);
                }
                self.recent_files.insert(0, path_buf.clone());
                if self.recent_files.len() > 10 {
                    self.recent_files.truncate(10);
                }

                let dur = start_time.elapsed();
                let engine = Arc::new(raw_engine);
                let header = engine.read_range(0, 65536).unwrap_or(&[]);
                let file_type = FormatDetector::detect(header, engine.detect_encoding(), Some(engine.path()));

                let line_index = Arc::new(LineIndex::new(engine.size(), engine.detect_encoding()));
                let cancel = Arc::new(AtomicBool::new(false));

                // Spawn background indexer thread
                let handle = LineIndexer::spawn(Arc::clone(&engine), Arc::clone(&line_index), Arc::clone(&cancel));

                self.current_line = 1;
                self.viewport.load_lines_indexed(&engine, &line_index, 1, VISIBLE_LINE_BUFFER);

                self.document = Some(EditorDocument::new(engine.size()));
                self.is_edit_mode = false;
                self.active_edit_line = None;
                self.edit_line_buffer.clear();

                self.engine = Some(Arc::clone(&engine));
                self.line_index = Some(Arc::clone(&line_index));
                self.indexer_cancel = Some(Arc::clone(&cancel));
                self.indexer_handle = Some(handle);
                self.file_type = Some(file_type);
                self.open_duration = Some(dur);
                self.error_message = None;
                self.jump_line_input = "1".to_string();
                self.jump_offset_input = "0".to_string();

                self.search_matches.write().unwrap().clear();
                *self.search_status.write().unwrap() = SearchStatus::Idle;
                self.active_match_idx = None;
                self.last_executed_query = None;
                self.auto_jump_to_first_match = false;

                self.xml_tree_root = None;
                self.xml_tree_building = false;
                self.xml_tree_rx = None;
                self.xml_tree_cancel = None;
                self.xml_validation_result = None;
                self.is_validating_xml = false;

                self.json_tree_root = None;
                self.json_tree_building = false;
                self.json_tree_rx = None;
                self.json_tree_cancel = None;
                self.json_validation_result = None;
                self.is_validating_json = false;

                let tab_id = self.next_tab_id;
                self.next_tab_id += 1;
                self.active_tab_id = Some(tab_id);

                let new_tab = OpenDocState {
                    id: tab_id,
                    path: path_buf.clone(),
                    engine: Arc::clone(&engine),
                    line_index: Arc::clone(&line_index),
                    indexer_cancel: Arc::clone(&cancel),
                    indexer_handle: None,
                    viewport: self.viewport.clone(),
                    file_type: Some(file_type),
                    open_duration: Some(dur),
                    current_line: 1,
                    document: self.document.clone(),
                    is_edit_mode: false,
                    active_edit_line: None,
                    edit_line_buffer: String::new(),
                    xml_tree_root: None,
                    json_tree_root: None,
                };
                self.tabs.push(new_tab);

                // Auto-detect CSV/TSV
                let ext = path_buf.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "csv" {
                    self.csv_grid.is_enabled = true;
                    self.csv_grid.delimiter = ',';
                } else if ext == "tsv" {
                    self.csv_grid.is_enabled = true;
                    self.csv_grid.delimiter = '\t';
                } else {
                    self.csv_grid.is_enabled = false;
                }

                // Auto-build compact structure tree if size <= 2GB
                if file_type == FileType::Xml {
                    self.trigger_build_xml_tree();
                } else if file_type == FileType::Json {
                    self.trigger_build_json_tree();
                }

                self.persist_session();
            }
            Err(err) => {
                self.error_message = Some(format!("Failed to open file: {}", err));
            }
        }
    }

    pub fn persist_session(&mut self) {
        self.session.open_files = self.tabs.iter().map(|t| t.path.clone()).collect();
        self.session.recent_files = self.recent_files.clone();
        self.session.font_size = self.font_size;
        self.session.theme_name = self.current_theme.name().to_string();
        self.session.word_wrap = self.word_wrap;
        self.session.save();
    }

    pub fn set_theme(&mut self, theme: ColorTheme, ctx: &egui::Context) {
        self.current_theme = theme;
        apply_modern_theme(ctx, theme);
        self.persist_session();
    }

    pub fn sync_active_tab_state(&mut self) {
        if let Some(active_id) = self.active_tab_id {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
                if let Some(engine) = &self.engine {
                    tab.engine = Arc::clone(engine);
                }
                if let Some(line_index) = &self.line_index {
                    tab.line_index = Arc::clone(line_index);
                }
                if let Some(cancel) = &self.indexer_cancel {
                    tab.indexer_cancel = Arc::clone(cancel);
                }
                tab.viewport = self.viewport.clone();
                tab.file_type = self.file_type;
                tab.open_duration = self.open_duration;
                tab.current_line = self.current_line;
                tab.document = self.document.clone();
                tab.is_edit_mode = self.is_edit_mode;
                tab.active_edit_line = self.active_edit_line;
                tab.edit_line_buffer = self.edit_line_buffer.clone();
                tab.xml_tree_root = self.xml_tree_root.clone();
                tab.json_tree_root = self.json_tree_root.clone();
            }
        }
    }

    pub fn switch_to_tab(&mut self, tab_id: usize) {
        if self.active_tab_id == Some(tab_id) {
            return;
        }
        self.sync_active_tab_state();

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            self.engine = Some(Arc::clone(&tab.engine));
            self.line_index = Some(Arc::clone(&tab.line_index));
            self.indexer_cancel = Some(Arc::clone(&tab.indexer_cancel));
            self.viewport = tab.viewport.clone();
            self.file_type = tab.file_type;
            self.open_duration = tab.open_duration;
            self.current_line = tab.current_line;
            self.document = tab.document.clone();
            self.is_edit_mode = tab.is_edit_mode;
            self.active_edit_line = tab.active_edit_line;
            self.edit_line_buffer = tab.edit_line_buffer.clone();
            self.xml_tree_root = tab.xml_tree_root.clone();
            self.json_tree_root = tab.json_tree_root.clone();

            self.search_matches.write().unwrap().clear();
            *self.search_status.write().unwrap() = SearchStatus::Idle;
            self.active_match_idx = None;

            let ext = tab.path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if ext == "csv" {
                self.csv_grid.is_enabled = true;
                self.csv_grid.delimiter = ',';
            } else if ext == "tsv" {
                self.csv_grid.is_enabled = true;
                self.csv_grid.delimiter = '\t';
            } else {
                self.csv_grid.is_enabled = false;
            }
        }
    }

    pub fn close_tab_by_id(&mut self, tab_id: usize) {
        let pos = self.tabs.iter().position(|t| t.id == tab_id);
        if let Some(idx) = pos {
            let is_active = self.active_tab_id == Some(tab_id);
            let tab = self.tabs.remove(idx);
            tab.indexer_cancel.store(true, Ordering::Relaxed);

            if is_active {
                if let Some(next_tab) = self.tabs.get(idx.min(self.tabs.len().saturating_sub(1))) {
                    let next_id = next_tab.id;
                    self.active_tab_id = None;
                    self.switch_to_tab(next_id);
                } else {
                    self.active_tab_id = None;
                    self.do_close_file();
                }
            }
            self.persist_session();
        }
    }

    pub fn close_all_tabs(&mut self) {
        for tab in self.tabs.drain(..) {
            tab.indexer_cancel.store(true, Ordering::Relaxed);
        }
        self.active_tab_id = None;
        self.do_close_file();
        self.persist_session();
    }

    pub fn trigger_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.folder_explorer.root_path = Some(folder);
            self.active_activity_panel = ActivityPanel::Explorer;
        }
    }

    pub fn close_file(&mut self) {
        if self.document.as_ref().map_or(false, |d| d.is_dirty()) {
            self.pending_file_action = Some(PendingFileAction::Close);
            self.show_unsaved_dialog = true;
            return;
        }
        if let Some(active_id) = self.active_tab_id {
            self.close_tab_by_id(active_id);
        } else {
            self.do_close_file();
        }
    }

    pub fn do_close_file(&mut self) {
        self.cancel_active_indexer();
        self.cancel_search();
        self.cancel_tree_builders();
        self.cancel_analysis();
        self.analyzer_state = AnalyzerPanelState::default();
        self.engine = None;
        self.line_index = None;
        self.viewport = Viewport::new();
        self.file_type = None;
        self.open_duration = None;
        self.current_line = 1;
        self.jump_line_input.clear();
        self.jump_offset_input.clear();
        self.error_message = None;
        self.show_search_results = false;
        self.last_executed_query = None;
        self.auto_jump_to_first_match = false;
        self.xml_tree_root = None;
        self.xml_validation_result = None;
        self.json_tree_root = None;
        self.json_validation_result = None;
        self.document = None;
        self.is_edit_mode = false;
        self.active_edit_line = None;
        self.edit_line_buffer.clear();
    }

    fn cancel_tree_builders(&mut self) {
        if let Some(cancel) = self.xml_tree_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.xml_tree_rx = None;
        self.xml_tree_building = false;

        if let Some(cancel) = self.json_tree_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.json_tree_rx = None;
        self.json_tree_building = false;
    }

    fn cancel_active_indexer(&mut self) {
        if let Some(cancel) = self.indexer_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.indexer_handle = None;
    }

    pub fn start_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        self.cancel_search();
        self.search_matches.write().unwrap().clear();
        self.active_match_idx = None;
        self.last_executed_query = Some(self.search_query.clone());
        self.auto_jump_to_first_match = true;

        if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
            let cancel = Arc::new(AtomicBool::new(false));
            *self.search_status.write().unwrap() = SearchStatus::Searching {
                progress_pct: 0.0,
                matches_found: 0,
                speed_mb_s: 0,
            };

            let handle = SearchWorker::spawn(
                Arc::clone(engine),
                Arc::clone(index),
                self.search_query.clone(),
                Arc::clone(&self.search_matches),
                Arc::clone(&self.search_status),
                Arc::clone(&cancel),
            );

            self.search_cancel = Some(cancel);
            self.search_handle = Some(handle);
        }
    }

    pub fn cancel_search(&mut self) {
        if let Some(cancel) = self.search_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.search_handle = None;
        self.auto_jump_to_first_match = false;
    }

    pub fn find_next(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        let query_changed = self.last_executed_query.as_ref() != Some(&self.search_query);
        let matches_count = self.search_matches.read().unwrap().len();

        if query_changed || matches_count == 0 {
            self.start_search();
            return;
        }

        let next_idx = match self.active_match_idx {
            Some(curr) => (curr + 1) % matches_count,
            None => 0,
        };
        self.select_match(next_idx);
    }

    pub fn find_prev(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        let query_changed = self.last_executed_query.as_ref() != Some(&self.search_query);
        let matches_count = self.search_matches.read().unwrap().len();

        if query_changed || matches_count == 0 {
            self.start_search();
            return;
        }

        let prev_idx = match self.active_match_idx {
            Some(curr) => {
                if curr == 0 {
                    matches_count.saturating_sub(1)
                } else {
                    curr - 1
                }
            }
            None => matches_count.saturating_sub(1),
        };
        self.select_match(prev_idx);
    }

    pub fn select_match(&mut self, idx: usize) {
        let matches = self.search_matches.read().unwrap();
        if let Some(m) = matches.get(idx) {
            let line = m.line_number;
            self.active_match_idx = Some(idx);
            drop(matches);
            self.scroll_to_line(line);
        }
    }

    pub fn set_search_pattern(&mut self, pattern: &str) {
        self.search_query.pattern = pattern.to_string();
    }

    pub fn set_search_case_sensitive(&mut self, case_sensitive: bool) {
        self.search_query.case_sensitive = case_sensitive;
    }

    pub fn active_match_index(&self) -> Option<usize> {
        self.active_match_idx
    }

    pub fn search_matches_count(&self) -> usize {
        let status = self.search_status.read().unwrap();
        match *status {
            SearchStatus::Searching { matches_found, .. }
            | SearchStatus::Completed { matches_found, .. }
            | SearchStatus::Cancelled { matches_found } => matches_found,
            _ => self.search_matches.read().unwrap().len(),
        }
    }

    pub fn current_line(&self) -> usize {
        self.current_line
    }

    pub fn open_search_bar(&mut self) {
        self.show_search_bar = true;
        self.focus_search_input = true;
    }

    pub fn close_search_bar(&mut self) {
        self.show_search_bar = false;
    }

    pub fn is_search_bar_visible(&self) -> bool {
        self.show_search_bar
    }

    pub fn trigger_build_xml_tree(&mut self) {
        if self.xml_tree_building {
            return;
        }
        if let Some(ref engine) = self.engine {
            self.xml_tree_building = true;
            let engine_clone = Arc::clone(engine);
            let cancel = Arc::new(AtomicBool::new(false));
            self.xml_tree_cancel = Some(Arc::clone(&cancel));

            let (tx, rx) = crossbeam_channel::bounded(1);
            self.xml_tree_rx = Some(rx);

            std::thread::Builder::new()
                .name("xml-tree-builder".to_string())
                .spawn(move || {
                    let tree = XmlStructureIndexer::build_tree(engine_clone, cancel);
                    let _ = tx.send(tree);
                })
                .expect("Failed to spawn XML tree builder");
        }
    }

    pub fn validate_xml_document(&mut self) {
        if let Some(ref engine) = self.engine {
            self.is_validating_xml = true;
            let engine_clone = Arc::clone(engine);
            let cancel = Arc::new(AtomicBool::new(false));

            let res = XmlValidator::validate(engine_clone, cancel);
            self.xml_validation_result = Some(res);
            self.is_validating_xml = false;
        }
    }

    pub fn trigger_build_json_tree(&mut self) {
        if self.json_tree_building {
            return;
        }
        if let Some(ref engine) = self.engine {
            self.json_tree_building = true;
            let engine_clone = Arc::clone(engine);
            let cancel = Arc::new(AtomicBool::new(false));
            self.json_tree_cancel = Some(Arc::clone(&cancel));

            let (tx, rx) = crossbeam_channel::bounded(1);
            self.json_tree_rx = Some(rx);

            std::thread::Builder::new()
                .name("json-tree-builder".to_string())
                .spawn(move || {
                    let tree = JsonStructureIndexer::build_tree(engine_clone, cancel);
                    let _ = tx.send(tree);
                })
                .expect("Failed to spawn JSON tree builder");
        }
    }

    pub fn validate_json_document(&mut self) {
        if let Some(ref engine) = self.engine {
            self.is_validating_json = true;
            let engine_clone = Arc::clone(engine);
            let cancel = Arc::new(AtomicBool::new(false));

            let res = JsonValidator::validate(engine_clone, cancel);
            self.json_validation_result = Some(res);
            self.is_validating_json = false;
        }
    }

    pub fn start_formatting(&mut self, action: FormatAction, in_place: bool, custom_dst: Option<PathBuf>) {
        if self.active_formatting.is_some() {
            return;
        }

        let engine = match self.engine {
            Some(ref e) => Arc::clone(e),
            None => return,
        };

        let file_type = match self.file_type {
            Some(t) => t,
            None => return,
        };

        let src_path = engine.path().to_path_buf();
        let (dst_path, is_in_place) = if let Some(dst) = custom_dst {
            (dst, false)
        } else if in_place {
            let tmp_name = format!("{}.tmp_formatted", src_path.display());
            (PathBuf::from(tmp_name), true)
        } else {
            return;
        };

        let title = match (file_type, action) {
            (FileType::Json, FormatAction::Beautify { indent_size, .. }) => {
                format!("Beautifying JSON Document ({} Spaces)", indent_size)
            }
            (FileType::Json, FormatAction::Minify) => "Minifying JSON Document (Compact)".to_string(),
            (FileType::Xml, FormatAction::Beautify { indent_size, .. }) => {
                format!("Beautifying XML Document ({} Spaces)", indent_size)
            }
            (FileType::Xml, FormatAction::Minify) => "Minifying XML Document (Compact)".to_string(),
            _ => "Formatting Document".to_string(),
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(FormattingProgress::new(engine.size())));

        let cancel_thread = Arc::clone(&cancel);
        let progress_thread = Arc::clone(&progress);
        let src_thread = src_path.clone();
        let dst_thread = dst_path.clone();

        std::thread::Builder::new()
            .name("streaming-formatter".to_string())
            .spawn(move || {
                let res = match file_type {
                    FileType::Json => JsonStreamingFormatter::format_file(
                        src_thread,
                        dst_thread,
                        action,
                        cancel_thread,
                        Some(progress_thread),
                    ),
                    FileType::Xml => XmlStreamingFormatter::format_file(
                        src_thread,
                        dst_thread,
                        action,
                        cancel_thread,
                        Some(progress_thread),
                    ),
                    _ => Err("Unsupported file format for streaming formatting".to_string()),
                };
                if let Err(e) = res {
                    eprintln!("Formatting error: {}", e);
                }
            })
            .expect("Failed to spawn formatter thread");

        self.active_formatting = Some(ActiveFormatting {
            title,
            action,
            src_path,
            dst_path,
            is_in_place,
            progress,
            cancel,
        });
        self.show_format_modal = true;
    }

    pub fn cancel_formatting(&mut self) {
        if let Some(ref active) = self.active_formatting {
            active.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn trigger_format_save_as(&mut self) {
        if let Some(ref engine) = self.engine {
            let ext = engine.path().extension().and_then(|e| e.to_str()).unwrap_or("txt");
            let base_name = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("formatted");
            let suggested = format!("{}_formatted.{}", base_name, ext);

            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Formatted Document As")
                .set_file_name(&suggested)
                .save_file()
            {
                self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, false, Some(path));
            }
        }
    }

    pub fn start_analysis(&mut self) {
        let Some(ref engine) = self.engine else {
            return;
        };
        let src_path = engine.path().to_path_buf();
        let file_type = self.file_type.unwrap_or(FileType::PlainText);
        let total_bytes = engine.size();

        self.cancel_analysis();
        self.analyzer_state.reset_for_new_analysis();

        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(FormattingProgress::new(total_bytes)));
        self.analyzer_state.progress = Some(Arc::clone(&progress));

        let cancel_thread = Arc::clone(&cancel);
        let progress_thread = Arc::clone(&progress);
        let (tx, rx) = crossbeam_channel::bounded(1);

        std::thread::Builder::new()
            .name("field-analyzer".to_string())
            .spawn(move || {
                let res = StreamingFieldAnalyzer::analyze_file(
                    src_path,
                    file_type,
                    cancel_thread,
                    Some(progress_thread),
                );
                let _ = tx.send(res);
            })
            .expect("Failed to spawn field analyzer thread");

        self.active_analysis = Some(ActiveAnalysis { cancel, rx });
    }

    pub fn cancel_analysis(&mut self) {
        if let Some(ref active) = self.active_analysis {
            active.cancel.store(true, Ordering::SeqCst);
        }
        self.active_analysis = None;
    }

    pub fn export_analysis_schema(&mut self) {
        if let Some(ref report) = self.analyzer_state.report {
            let default_name = if let Some(ref engine) = self.engine {
                let stem = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("schema");
                format!("{}_schema.json", stem)
            } else {
                "schema.json".to_string()
            };

            if let Some(save_path) = rfd::FileDialog::new()
                .set_title("Export Analysis Schema JSON")
                .set_file_name(&default_name)
                .save_file()
            {
                let json_data = report.to_json_pretty();
                if let Err(e) = std::fs::write(&save_path, json_data) {
                    self.analyzer_state.status_msg = Some(format!("Failed to export schema: {}", e));
                } else {
                    self.analyzer_state.status_msg = Some(format!("Schema successfully saved to {:?}", save_path));
                }
            }
        }
    }

    pub fn commit_line_edit(&mut self, line_number: usize, byte_offset: u64, old_text: &str) {
        let new_text = self.edit_line_buffer.clone();
        if let Some(ref mut doc) = self.document {
            doc.edit_line(line_number, byte_offset, old_text, &new_text);
        }
        for line in &mut self.viewport.lines {
            if line.line_number == line_number {
                line.text = new_text.clone();
                break;
            }
        }
        self.active_edit_line = None;
    }

    pub fn undo(&mut self) {
        if let Some(ref mut doc) = self.document {
            if doc.undo() {
                if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
                    self.viewport.load_lines_indexed(engine, index, self.current_line, VISIBLE_LINE_BUFFER);
                    self.viewport.apply_line_overrides(&doc.modified_lines);
                }
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(ref mut doc) = self.document {
            if doc.redo() {
                if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
                    self.viewport.load_lines_indexed(engine, index, self.current_line, VISIBLE_LINE_BUFFER);
                    self.viewport.apply_line_overrides(&doc.modified_lines);
                }
            }
        }
    }

    pub fn start_save_in_place(&mut self) {
        let (Some(ref engine), Some(ref doc)) = (&self.engine, &self.document) else {
            return;
        };
        if !doc.is_dirty() {
            return;
        }

        let doc_clone = doc.clone();
        let engine_clone = Arc::clone(engine);
        let src_path = engine.path().to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(FormattingProgress::new(doc.piece_table.total_length())));

        let cancel_thread = Arc::clone(&cancel);
        let progress_thread = Arc::clone(&progress);
        let (tx, rx) = crossbeam_channel::bounded(1);

        std::thread::Builder::new()
            .name("atomic-saver".to_string())
            .spawn(move || {
                let res = SaveManager::save_in_place(
                    &doc_clone,
                    &engine_clone,
                    cancel_thread,
                    Some(progress_thread),
                );
                let _ = tx.send(res);
            })
            .expect("Failed to spawn saver thread");

        self.active_saving = Some(ActiveSaving {
            title: format!("Saving {}...", src_path.file_name().and_then(|n| n.to_str()).unwrap_or("document")),
            target_path: src_path,
            is_in_place: true,
            progress,
            cancel,
            rx,
        });
        self.show_save_modal = true;
    }

    pub fn start_save_as(&mut self) {
        let (Some(ref engine), Some(ref doc)) = (&self.engine, &self.document) else {
            return;
        };

        let ext = engine.path().extension().and_then(|e| e.to_str()).unwrap_or("txt");
        let base_name = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("document");
        let suggested = format!("{}_saved.{}", base_name, ext);

        let Some(target_path) = rfd::FileDialog::new()
            .set_title("Save Document As")
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };

        let doc_clone = doc.clone();
        let engine_clone = Arc::clone(engine);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(FormattingProgress::new(doc.piece_table.total_length())));

        let cancel_thread = Arc::clone(&cancel);
        let progress_thread = Arc::clone(&progress);
        let target_thread = target_path.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);

        std::thread::Builder::new()
            .name("save-as-worker".to_string())
            .spawn(move || {
                let res = SaveManager::save_as(
                    &doc_clone,
                    &engine_clone,
                    &target_thread,
                    cancel_thread,
                    Some(progress_thread),
                ).map(|_| target_thread);
                let _ = tx.send(res);
            })
            .expect("Failed to spawn save-as thread");

        self.active_saving = Some(ActiveSaving {
            title: format!("Saving as {:?}...", target_path.file_name().and_then(|n| n.to_str()).unwrap_or("document")),
            target_path,
            is_in_place: false,
            progress,
            cancel,
            rx,
        });
        self.show_save_modal = true;
    }

    pub fn cancel_saving(&mut self) {
        if let Some(ref active) = self.active_saving {
            active.cancel.store(true, Ordering::SeqCst);
        }
        self.active_saving = None;
    }

    pub fn scroll_to_line(&mut self, target_line: usize) {
        if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
            let max_line = index.total_lines().max(1);
            let clamped = target_line.clamp(1, max_line);
            self.current_line = clamped;
            self.viewport.load_lines_indexed(engine, index, clamped, VISIBLE_LINE_BUFFER);
            if let Some(ref doc) = self.document {
                self.viewport.apply_line_overrides(&doc.modified_lines);
            }
            self.jump_line_input = clamped.to_string();
            if let Some(offset) = self.viewport.lines.first().map(|l| l.byte_offset) {
                self.jump_offset_input = offset.to_string();
            }
        }
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        if self.engine.is_some() {
            let new_line = if delta < 0 {
                self.current_line.saturating_sub((-delta) as usize).max(1)
            } else {
                self.current_line.saturating_add(delta as usize)
            };
            self.scroll_to_line(new_line);
        }
    }

    pub fn scroll_to_offset(&mut self, offset: u64) {
        if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
            let line = index.byte_offset_to_line(engine, offset);
            self.scroll_to_line(line);
        }
    }

    fn update_memory_metrics(&mut self) {
        if self.last_sys_refresh.elapsed() >= Duration::from_millis(500) {
            self.last_sys_refresh = Instant::now();
            if let Some(pid) = self.current_pid {
                self.system_info.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
                if let Some(proc) = self.system_info.process(pid) {
                    self.cached_rss_bytes = proc.memory();
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (ctrl_o, ctrl_w, ctrl_s, ctrl_shift_s, ctrl_z, ctrl_y, ctrl_e, ctrl_f, ctrl_g, ctrl_shift_e, ctrl_shift_f, ctrl_shift_t, ctrl_shift_b, ctrl_shift_m, ctrl_shift_a, ctrl_shift_p, ctrl_comma, f1, f3, shift_f3, esc, zoom_in, zoom_out, zoom_reset, up, down, page_up, page_down, home, end, alt_z) = ctx.input(|i| {
            let ctrl = i.modifiers.command;
            let shift = i.modifiers.shift;
            let alt = i.modifiers.alt;
            (
                ctrl && !shift && i.key_pressed(Key::O),
                ctrl && !shift && i.key_pressed(Key::W),
                ctrl && !shift && i.key_pressed(Key::S),
                ctrl && shift && i.key_pressed(Key::S),
                ctrl && !shift && i.key_pressed(Key::Z),
                ctrl && !shift && i.key_pressed(Key::Y),
                ctrl && !shift && i.key_pressed(Key::E),
                ctrl && !shift && i.key_pressed(Key::F),
                ctrl && !shift && i.key_pressed(Key::G),
                ctrl && shift && i.key_pressed(Key::E),
                ctrl && shift && i.key_pressed(Key::F),
                ctrl && shift && i.key_pressed(Key::T),
                ctrl && shift && i.key_pressed(Key::B),
                ctrl && shift && i.key_pressed(Key::M),
                ctrl && shift && i.key_pressed(Key::A),
                ctrl && shift && i.key_pressed(Key::P),
                ctrl && !shift && i.key_pressed(Key::Comma),
                i.key_pressed(Key::F1),
                i.key_pressed(Key::F3) && !shift,
                i.key_pressed(Key::F3) && shift,
                i.key_pressed(Key::Escape),
                ctrl && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)),
                ctrl && i.key_pressed(Key::Minus),
                ctrl && i.key_pressed(Key::Num0),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::PageUp),
                i.key_pressed(Key::PageDown),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                alt && !ctrl && i.key_pressed(Key::Z),
            )
        });

        if ctrl_o {
            self.trigger_file_dialog();
        }
        if ctrl_w {
            self.close_file();
        }
        if ctrl_shift_p {
            self.command_palette.is_open = !self.command_palette.is_open;
            self.command_palette.query.clear();
        }
        if ctrl_s && self.engine.is_some() {
            self.start_save_in_place();
        }
        if ctrl_shift_s && self.engine.is_some() {
            self.start_save_as();
        }
        if ctrl_z && self.engine.is_some() {
            self.undo();
        }
        if ctrl_y && self.engine.is_some() {
            self.redo();
        }
        if ctrl_e && self.engine.is_some() {
            self.is_edit_mode = !self.is_edit_mode;
            if !self.is_edit_mode {
                self.active_edit_line = None;
            }
        }
        let ctrl_b = ctx.input(|i| i.modifiers.command && i.key_pressed(Key::B));
        if ctrl_b || ctrl_shift_e {
            self.active_activity_panel = match self.active_activity_panel {
                ActivityPanel::Explorer => ActivityPanel::None,
                _ => ActivityPanel::Explorer,
            };
        }
        if ctrl_shift_t && self.engine.is_some() {
            self.active_activity_panel = match self.active_activity_panel {
                ActivityPanel::Structure => ActivityPanel::None,
                _ => {
                    let is_xml = self.file_type == Some(FileType::Xml);
                    let is_json = self.file_type == Some(FileType::Json);
                    if is_xml && self.xml_tree_root.is_none() {
                        self.trigger_build_xml_tree();
                    } else if is_json && self.json_tree_root.is_none() {
                        self.trigger_build_json_tree();
                    }
                    ActivityPanel::Structure
                }
            };
        }
        if alt_z {
            self.word_wrap = !self.word_wrap;
        }
        if ctrl_f && self.engine.is_some() {
            self.show_search_bar = true;
            self.focus_search_input = true;
        }
        if ctrl_shift_f && self.engine.is_some() {
            self.active_activity_panel = match self.active_activity_panel {
                ActivityPanel::Search => ActivityPanel::None,
                _ => ActivityPanel::Search,
            };
        }
        if ctrl_g && self.engine.is_some() {
            self.show_goto_line_dialog = true;
        }
        if ctrl_shift_b && self.engine.is_some() {
            self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, true, None);
        }
        if ctrl_shift_m && self.engine.is_some() {
            self.start_formatting(FormatAction::Minify, true, None);
        }
        if ctrl_shift_a && self.engine.is_some() {
            self.active_activity_panel = match self.active_activity_panel {
                ActivityPanel::Analyzer => ActivityPanel::None,
                _ => {
                    self.start_analysis();
                    ActivityPanel::Analyzer
                }
            };
        }
        if ctrl_comma {
            self.show_about_dialog = !self.show_about_dialog;
        }
        if f1 {
            self.show_about_dialog = !self.show_about_dialog;
        }
        if f3 && self.engine.is_some() {
            self.find_next();
        }
        if shift_f3 && self.engine.is_some() {
            self.find_prev();
        }
        if esc {
            if self.analyzer_state.is_open {
                self.cancel_analysis();
                self.analyzer_state.is_open = false;
            } else if self.show_format_modal {
                self.show_format_modal = false;
            } else if self.show_goto_line_dialog {
                self.show_goto_line_dialog = false;
            } else if self.show_search_results {
                self.show_search_results = false;
            } else if self.show_search_bar {
                self.show_search_bar = false;
            } else if self.active_activity_panel != ActivityPanel::None {
                self.active_activity_panel = ActivityPanel::None;
            }
        }

        if zoom_in {
            self.font_size = (self.font_size + 1.0).min(32.0);
        }
        if zoom_out {
            self.font_size = (self.font_size - 1.0).max(8.0);
        }
        if zoom_reset {
            self.font_size = 14.0;
        }

        if up {
            self.scroll_lines(-1);
        }
        if down {
            self.scroll_lines(1);
        }
        if page_up {
            self.scroll_lines(-(VISIBLE_LINE_BUFFER as isize));
        }
        if page_down {
            self.scroll_lines(VISIBLE_LINE_BUFFER as isize);
        }
        if home {
            self.scroll_to_line(1);
        }
        if end {
            if let Some(ref index) = self.line_index {
                let total = index.total_lines();
                self.scroll_to_line(total.saturating_sub(VISIBLE_LINE_BUFFER).max(1));
            }
        }

        let (scroll_y, is_ctrl) = ctx.input(|i| (i.raw_scroll_delta.y, i.modifiers.command || i.modifiers.ctrl));
        if is_ctrl && scroll_y.abs() > 0.1 {
            if scroll_y > 0.0 {
                self.font_size = (self.font_size + 1.0).min(36.0);
            } else {
                self.font_size = (self.font_size - 1.0).max(8.0);
            }
        } else if scroll_y.abs() > 0.1 {
            let lines_to_scroll = (-(scroll_y / 15.0)).round() as isize;
            if lines_to_scroll != 0 {
                self.scroll_lines(lines_to_scroll);
            }
        }
    }

    fn handle_drag_and_drop(&mut self, ctx: &egui::Context) {
        let dropped_file = ctx.input(|i| {
            i.raw.dropped_files.first().and_then(|f| f.path.clone())
        });

        if let Some(path) = dropped_file {
            self.open_file(path);
        }
    }

    fn handle_window_resizing(&self, ctx: &egui::Context) {
        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if is_maximized {
            return;
        }

        let screen_rect = ctx.screen_rect();
        let border = 6.0;
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
        let mouse_down = ctx.input(|i| i.pointer.primary_down());

        if let Some(pos) = pointer_pos {
            let on_left = pos.x <= screen_rect.left() + border;
            let on_right = pos.x >= screen_rect.right() - border;
            let on_top = pos.y <= screen_rect.top() + border;
            let on_bottom = pos.y >= screen_rect.bottom() - border;

            let direction = match (on_top, on_bottom, on_left, on_right) {
                (true, false, true, false) => Some(egui::ResizeDirection::NorthWest),
                (true, false, false, true) => Some(egui::ResizeDirection::NorthEast),
                (false, true, true, false) => Some(egui::ResizeDirection::SouthWest),
                (false, true, false, true) => Some(egui::ResizeDirection::SouthEast),
                (true, false, false, false) => Some(egui::ResizeDirection::North),
                (false, true, false, false) => Some(egui::ResizeDirection::South),
                (false, false, true, false) => Some(egui::ResizeDirection::West),
                (false, false, false, true) => Some(egui::ResizeDirection::East),
                _ => None,
            };

            if let Some(dir) = direction {
                let cursor = match dir {
                    egui::ResizeDirection::North | egui::ResizeDirection::South => egui::CursorIcon::ResizeVertical,
                    egui::ResizeDirection::East | egui::ResizeDirection::West => egui::CursorIcon::ResizeHorizontal,
                    egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthEast => egui::CursorIcon::ResizeNwSe,
                    egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthWest => egui::CursorIcon::ResizeNeSw,
                };
                ctx.set_cursor_icon(cursor);

                if mouse_down && ctx.input(|i| i.pointer.primary_pressed()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            }
        }
    }

    fn trigger_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open Large File - UltraViewer")
            .pick_file()
        {
            self.open_file(path);
        }
    }
}

impl eframe::App for UltraViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.style().visuals.panel_fill != self.current_theme.bg_color() {
            apply_modern_theme(ctx, self.current_theme);
        }

        if self.titlebar_applied_frames < 5 {
            super::win32_titlebar::apply_dark_title_bar();
            self.titlebar_applied_frames += 1;
        }

        let is_searching = matches!(*self.search_status.read().unwrap(), SearchStatus::Searching { .. });
        let is_indexing = self.line_index.as_ref().map_or(false, |i| !i.is_complete());

        if is_indexing || is_searching || self.xml_tree_building || self.is_validating_xml || self.json_tree_building || self.is_validating_json || self.active_analysis.is_some() || self.active_saving.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        // Auto-navigate to first match when background search finds results
        if self.auto_jump_to_first_match {
            let has_matches = {
                let matches = self.search_matches.read().unwrap();
                !matches.is_empty()
            };
            if has_matches {
                self.auto_jump_to_first_match = false;
                self.select_match(0);
            } else {
                let status = self.search_status.read().unwrap();
                if matches!(*status, SearchStatus::Completed { .. } | SearchStatus::Cancelled { .. } | SearchStatus::Error(_)) {
                    self.auto_jump_to_first_match = false;
                }
            }
        }

        // Poll background JSON structure tree builder
        if let Some(ref rx) = self.json_tree_rx {
            match rx.try_recv() {
                Ok(tree) => {
                    self.json_tree_root = Some(tree);
                    self.json_tree_building = false;
                    self.json_tree_rx = None;
                    self.json_tree_cancel = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.json_tree_building = false;
                    self.json_tree_rx = None;
                    self.json_tree_cancel = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        // Poll background XML structure tree builder
        if let Some(ref rx) = self.xml_tree_rx {
            match rx.try_recv() {
                Ok(tree) => {
                    self.xml_tree_root = tree.map(Arc::new);
                    self.xml_tree_building = false;
                    self.xml_tree_rx = None;
                    self.xml_tree_cancel = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.xml_tree_building = false;
                    self.xml_tree_rx = None;
                    self.xml_tree_cancel = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        // Poll background Field Analyzer
        if let Some(ref active) = self.active_analysis {
            match active.rx.try_recv() {
                Ok(Ok(report)) => {
                    self.analyzer_state.set_finished(report);
                    self.active_analysis = None;
                    ctx.request_repaint();
                }
                Ok(Err(err)) => {
                    if err.contains("cancelled") {
                        self.analyzer_state.set_cancelled();
                    } else {
                        self.analyzer_state.set_error(err);
                    }
                    self.active_analysis = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.analyzer_state.set_error("Analyzer worker disconnected unexpectedly.".to_string());
                    self.active_analysis = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        // Poll background Saving Worker
        let mut save_finished_result = None;
        if let Some(ref active) = self.active_saving {
            match active.rx.try_recv() {
                Ok(res) => {
                    save_finished_result = Some(res);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    save_finished_result = Some(Err("Saver worker disconnected unexpectedly.".to_string()));
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        if let Some(res) = save_finished_result {
            let active = self.active_saving.take().unwrap();
            match res {
                Ok(path) => {
                    if let Some(ref mut doc) = self.document {
                        doc.mark_saved();
                    }
                    self.show_save_modal = false;
                    // If in-place, drop engine first to release file lock, then atomic rename
                    let final_path = if active.is_in_place {
                        let orig_path = active.target_path.clone();
                        self.engine = None; // Unmap file
                        if let Err(e) = std::fs::rename(&path, &orig_path) {
                            self.error_message = Some(format!("Failed to finalize in-place save: {}", e));
                            orig_path
                        } else {
                            orig_path
                        }
                    } else {
                        path
                    };

                    self.do_open_file(&final_path);
                    if let Some(pending) = self.pending_file_action.take() {
                        match pending {
                            PendingFileAction::Open(p) => self.do_open_file(&p),
                            PendingFileAction::Close => self.do_close_file(),
                            PendingFileAction::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        }
                    }
                }
                Err(err) => {
                    self.show_save_modal = false;
                    if !err.contains("cancelled") {
                        self.error_message = Some(format!("Save failed: {}", err));
                    }
                    self.pending_file_action = None;
                }
            }
            ctx.request_repaint();
        }

        self.update_memory_metrics();
        self.handle_shortcuts(ctx);
        self.handle_drag_and_drop(ctx);
        self.handle_window_resizing(ctx);

        let has_file = self.engine.is_some();
        let is_xml = self.file_type == Some(FileType::Xml);
        let is_json = self.file_type == Some(FileType::Json);
        let is_dirty = self.document.as_ref().map_or(false, |d| d.is_dirty());
        let can_undo = self.document.as_ref().map_or(false, |d| d.can_undo());
        let can_redo = self.document.as_ref().map_or(false, |d| d.can_redo());
        let is_edit_mode = self.is_edit_mode;

        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        // 1. Top Menu Bar (Full width across the window, starting with Logo followed by menus)
        let menu_action = egui::TopBottomPanel::top("menu_bar")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(30, 34, 39))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31)))
                    .inner_margin(egui::Margin { left: 8, right: 0, top: 0, bottom: 0 }),
            )
            .show(ctx, |ui| {
                render_menu_bar(
                    ui,
                    has_file,
                    is_xml,
                    is_json,
                    is_dirty,
                    can_undo,
                    can_redo,
                    is_edit_mode,
                    self.font_size,
                    is_maximized,
                )
            })
            .inner;

        if let Some(action) = menu_action {
            match action {
                MenuAction::OpenFile => self.trigger_file_dialog(),
                MenuAction::CloseFile => self.close_file(),
                MenuAction::SaveFile => self.start_save_in_place(),
                MenuAction::SaveFileAs => self.start_save_as(),
                MenuAction::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                MenuAction::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                MenuAction::ToggleMaximize => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized)),
                MenuAction::Undo => self.undo(),
                MenuAction::Redo => self.redo(),
                MenuAction::ToggleEditMode => {
                    self.is_edit_mode = !self.is_edit_mode;
                    if !self.is_edit_mode {
                        self.active_edit_line = None;
                    }
                }
                MenuAction::Find => {
                    self.show_search_bar = true;
                    self.focus_search_input = true;
                }
                MenuAction::GoToLine => self.show_goto_line_dialog = true,
                MenuAction::ZoomIn => self.font_size = (self.font_size + 1.0).min(36.0),
                MenuAction::ZoomOut => self.font_size = (self.font_size - 1.0).max(8.0),
                MenuAction::ZoomReset => self.font_size = 14.0,
                MenuAction::ToggleTheme => {
                    let dark = !ctx.style().visuals.dark_mode;
                    if dark {
                        ctx.set_visuals(egui::Visuals::dark());
                    } else {
                        ctx.set_visuals(egui::Visuals::light());
                    }
                }
                MenuAction::XmlValidate => self.validate_xml_document(),
                MenuAction::XmlToggleTree => {
                    self.show_xml_tree = !self.show_xml_tree;
                    if self.show_xml_tree && self.xml_tree_root.is_none() {
                        self.trigger_build_xml_tree();
                    }
                }
                MenuAction::XmlToggleHighlight => self.enable_syntax_highlighting = !self.enable_syntax_highlighting,
                MenuAction::JsonValidate => self.validate_json_document(),
                MenuAction::JsonToggleTree => {
                    self.show_json_tree = !self.show_json_tree;
                    if self.show_json_tree && self.json_tree_root.is_none() {
                        self.trigger_build_json_tree();
                    }
                }
                MenuAction::JsonToggleHighlight => self.enable_syntax_highlighting = !self.enable_syntax_highlighting,
                MenuAction::FormatBeautify2 => {
                    self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, true, None);
                }
                MenuAction::FormatBeautify4 => {
                    self.start_formatting(FormatAction::Beautify { indent_size: 4, use_tabs: false }, true, None);
                }
                MenuAction::FormatMinify => {
                    self.start_formatting(FormatAction::Minify, true, None);
                }
                MenuAction::FormatSaveAs => {
                    self.trigger_format_save_as();
                }
                MenuAction::AnalyzeFields => {
                    self.start_analysis();
                }
                MenuAction::OpenFolder => self.trigger_folder_dialog(),
                MenuAction::CommandPalette => {
                    self.command_palette.is_open = true;
                    self.command_palette.query.clear();
                }
                MenuAction::ToggleCsvGrid => self.csv_grid.is_enabled = !self.csv_grid.is_enabled,
                MenuAction::OpenDiffViewer => self.diff_viewer.is_open = true,
                MenuAction::RegisterContextMenu => {
                    match ContextMenuManager::register() {
                        Ok(msg) => self.status_notification = Some((msg, Instant::now())),
                        Err(err) => self.error_message = Some(err),
                    }
                }
                MenuAction::UnregisterContextMenu => {
                    match ContextMenuManager::unregister() {
                        Ok(msg) => self.status_notification = Some((msg, Instant::now())),
                        Err(err) => self.error_message = Some(err),
                    }
                }
                MenuAction::SetTheme(t) => self.set_theme(t, ctx),
                MenuAction::About => self.show_about_dialog = true,
            }
        }

        // 2. Bottom Status Bar (Full width across the window)
        let file_name = self.engine.as_ref().and_then(|e| e.path().file_name().and_then(|n| n.to_str()));
        let file_size = self.engine.as_ref().map(|e| e.size()).unwrap_or(0);
        let encoding = self.engine.as_ref().map(|e| e.detect_encoding());
        let (indexing_pct, is_complete, speed, total_lines) = if let Some(ref idx) = self.line_index {
            (Some(idx.progress_pct()), idx.is_complete(), idx.speed_mb_s(), idx.total_lines())
        } else {
            (None, false, 0, 0)
        };
        let current_byte_offset = self.viewport.lines.first().map(|l| l.byte_offset).unwrap_or(0);
        let edit_count = self.document.as_ref().map_or(0, |d| d.edit_count());

        let status_action = egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
            .show(ctx, |ui| {
                render_status_bar(
                    ui,
                    StatusBarProps {
                        file_name,
                        file_size,
                        encoding,
                        file_type: self.file_type,
                        visible_lines_count: self.viewport.lines.len(),
                        current_line: self.current_line,
                        current_offset: current_byte_offset,
                        memory_rss_bytes: self.cached_rss_bytes,
                        open_latency: self.open_duration,
                        indexing_pct,
                        is_indexing_complete: is_complete,
                        indexing_speed_mb: speed,
                        total_indexed_lines: total_lines,
                        is_dirty,
                        edit_count,
                        is_edit_mode,
                        font_size: self.font_size,
                    },
                )
            })
            .inner;

        if let Some(status_act) = status_action {
            match status_act {
                StatusBarAction::ResetZoom => self.font_size = 14.0,
            }
        }

        // 3. Left VS Code Activity Bar (Dedicated 48px rail with vector icons)
        egui::SidePanel::left("activity_bar")
            .exact_width(48.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
            .show(ctx, |ui| {
                let search_matches_count = self.search_matches_count();
                let props = ActivityBarProps {
                    active_panel: self.active_activity_panel,
                    has_file,
                    is_xml_or_json: is_xml || is_json,
                    is_xml,
                    is_json,
                    search_match_count: search_matches_count,
                    is_expanded: false,
                };
                if let Some(act) = render_activity_bar(ui, &props) {
                    match act {
                        ActivityBarAction::TogglePanel(panel) => {
                            self.active_activity_panel = if self.active_activity_panel == panel {
                                ActivityPanel::None
                            } else {
                                panel
                            };
                            if self.active_activity_panel == ActivityPanel::Structure {
                                if is_xml && self.xml_tree_root.is_none() {
                                    self.trigger_build_xml_tree();
                                } else if is_json && self.json_tree_root.is_none() {
                                    self.trigger_build_json_tree();
                                }
                            } else if self.active_activity_panel == ActivityPanel::Analyzer {
                                self.start_analysis();
                            }
                        }
                        ActivityBarAction::OpenFile => self.trigger_file_dialog(),
                        ActivityBarAction::FormatBeautify => {
                            self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, true, None);
                        }
                        ActivityBarAction::FormatMinify => {
                            self.start_formatting(FormatAction::Minify, true, None);
                        }
                        ActivityBarAction::Validate => {
                            if is_xml {
                                self.validate_xml_document();
                            } else if is_json {
                                self.validate_json_document();
                            }
                        }
                        ActivityBarAction::ToggleSettings => self.show_about_dialog = true,
                        ActivityBarAction::ToggleHelp => self.show_about_dialog = true,
                        ActivityBarAction::ToggleExpanded => {},
                    }
                }
            });

        // Left Explorer Drawer (if active)
        if self.active_activity_panel == ActivityPanel::Explorer {
            let mut close_drawer = false;
            let mut open_recent = None;
            let mut pick_folder = false;
            egui::SidePanel::left("activity_drawer")
                .resizable(true)
                .default_width(240.0)
                .width_range(200.0..=360.0)
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("EXPLORER & RECENT").strong().size(11.0).color(Color32::from_rgb(140, 155, 175)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (c_rect, c_resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                            if c_resp.hovered() {
                                ui.painter().rect_filled(c_rect, 3.0, Color32::from_rgb(45, 25, 30));
                            }
                            super::icons::paint_icon(ui.painter(), c_rect.shrink(3.0), super::icons::Icon::Close, Color32::from_rgb(130, 140, 155));
                            if c_resp.clicked() {
                                close_drawer = true;
                            }
                        });
                    });
                    ui.separator();
                    if let Some(folder_act) = render_folder_explorer(ui, &mut self.folder_explorer) {
                        match folder_act {
                            FolderAction::OpenFile(p) => open_recent = Some(p),
                            FolderAction::OpenFolderDialog => pick_folder = true,
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label(RichText::new("RECENT FILES").strong().size(10.5).color(Color32::from_rgb(110, 125, 145)));
                    ui.add_space(4.0);
                    if self.recent_files.is_empty() {
                        ui.label(RichText::new("No recent files").size(11.0).weak().italics());
                    } else {
                        for path in &self.recent_files {
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                            ui.horizontal(|ui| {
                                let (f_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                                super::icons::paint_icon(ui.painter(), f_rect, super::icons::Icon::File, Color32::from_rgb(140, 155, 175));
                                let btn = ui.selectable_label(false, RichText::new(name).size(12.0));
                                if btn.on_hover_text(path.display().to_string()).clicked() {
                                    open_recent = Some(path.clone());
                                }
                            });
                        }
                    }
                });

            if pick_folder {
                self.trigger_folder_dialog();
            }
            if let Some(path) = open_recent {
                self.open_file(&path);
            }
            if close_drawer {
                self.active_activity_panel = ActivityPanel::None;
            }
        }

        // Right Structure Panel (XML/JSON Hierarchy Tree matching reference mockup media_1789811265859.jpg)
        if self.active_activity_panel == ActivityPanel::Structure && has_file {
            let mut close_drawer = false;
            let mut jump_offset = None;
            egui::SidePanel::left("structure_panel")
                .resizable(true)
                .default_width(280.0)
                .width_range(220.0..=450.0)
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    if is_xml {
                        render_xml_tree_panel(
                            ui,
                            self.xml_tree_root.as_deref(),
                            self.xml_tree_building,
                            &mut jump_offset,
                            &mut close_drawer,
                        );
                    } else if is_json {
                        render_json_tree_panel(
                            ui,
                            self.json_tree_root.as_deref(),
                            self.json_tree_building,
                            &mut jump_offset,
                            &mut close_drawer,
                        );
                    }
                });

            if let Some(offset) = jump_offset {
                self.scroll_to_offset(offset);
            }
            if close_drawer {
                self.active_activity_panel = ActivityPanel::None;
            }
        }



        // Navigation & Actions
        let mut action_open = false;
        let mut action_close = false;
        let mut action_toggle_search = false;
        let mut action_beautify = false;
        let mut action_minify = false;

        let mut action_toggle_edit = false;
        let mut action_save = false;

        let mut switch_tab_to = None;
        let mut close_tab_with_id = None;

        // Modern Tab Bar & Document Header (Option B)
        egui::TopBottomPanel::top("tab_bar_panel")
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ctx, |ui| {
                let file_size = self.engine.as_ref().map(|e| e.size()).unwrap_or(0);

                let tab_infos: Vec<TabInfo> = self.tabs.iter().map(|t| {
                    let name = t.path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    let is_dirty = t.document.as_ref().map_or(false, |d| d.is_dirty());
                    let is_active = self.active_tab_id == Some(t.id);
                    TabInfo {
                        id: t.id,
                        name,
                        file_type: t.file_type,
                        is_dirty,
                        is_active,
                    }
                }).collect();

                let tab_props = TabBarProps {
                    tabs: tab_infos,
                    file_size,
                    is_dirty,
                    is_edit_mode: self.is_edit_mode,
                    word_wrap: self.word_wrap,
                    current_line: self.current_line,
                    total_lines,
                    breadcrumb: None,
                };

                if let Some(tab_action) = render_tab_bar(ui, &tab_props) {
                    match tab_action {
                        TabBarAction::SelectTab(id) => switch_tab_to = Some(id),
                        TabBarAction::CloseTab(id) => close_tab_with_id = Some(id),
                        TabBarAction::OpenFile => action_open = true,
                        TabBarAction::CloseFile => action_close = true,
                        TabBarAction::ToggleEditMode => action_toggle_edit = true,
                        TabBarAction::SaveFile => action_save = true,
                        TabBarAction::ToggleWrap => self.word_wrap = !self.word_wrap,
                        TabBarAction::ToggleSearch => action_toggle_search = true,
                        TabBarAction::ToggleTree => {
                            self.active_activity_panel = match self.active_activity_panel {
                                ActivityPanel::Structure => ActivityPanel::None,
                                _ => ActivityPanel::Structure,
                            };
                            if is_xml && self.xml_tree_root.is_none() {
                                self.trigger_build_xml_tree();
                            } else if is_json && self.json_tree_root.is_none() {
                                self.trigger_build_json_tree();
                            }
                        }
                        TabBarAction::FormatBeautify => action_beautify = true,
                        TabBarAction::FormatMinify => action_minify = true,
                    }
                }
            });

        if let Some(id) = switch_tab_to {
            self.switch_to_tab(id);
        }
        if let Some(id) = close_tab_with_id {
            self.close_tab_by_id(id);
        }

        if action_open {
            self.trigger_file_dialog();
        }
        if action_close {
            self.close_file();
        }
        if action_toggle_edit {
            self.is_edit_mode = !self.is_edit_mode;
            if !self.is_edit_mode {
                self.active_edit_line = None;
            }
        }
        if action_save {
            self.start_save_in_place();
        }
        if action_toggle_search {
            self.show_search_bar = !self.show_search_bar;
            if self.show_search_bar {
                self.focus_search_input = true;
            }
        }
        if action_beautify {
            self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, true, None);
        }
        if action_minify {
            self.start_formatting(FormatAction::Minify, true, None);
        }

        // Search Bar Panel
        if self.show_search_bar && has_file {
            let status = self.search_status.read().unwrap().clone();
            let stored_matches = self.search_matches.read().unwrap().len();
            let actual_matches = self.search_matches_count();
            let req_focus = self.focus_search_input;
            if req_focus {
                self.focus_search_input = false;
            }

            egui::TopBottomPanel::top("search_bar_panel")
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
                .show(ctx, |ui| {
                if let Some(action) = render_search_bar(
                    ui,
                    &mut self.search_query,
                    &status,
                    self.active_match_idx,
                    stored_matches,
                    actual_matches,
                    req_focus,
                ) {
                    match action {
                        SearchBarAction::FindAll => {
                            self.show_search_results = true;
                            self.start_search();
                        }
                        SearchBarAction::FindNext => self.find_next(),
                        SearchBarAction::FindPrev => self.find_prev(),
                        SearchBarAction::Cancel => self.cancel_search(),
                        SearchBarAction::Close => self.show_search_bar = false,
                    }
                }
            });
        }

        // XML Validation Banner if available
        let mut dismiss_validation = false;
        if let Some(ref val_res) = self.xml_validation_result {
            egui::TopBottomPanel::top("xml_validation_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match val_res {
                        XmlValidationResult::Valid { elements_count, max_depth, elapsed_secs } => {
                            ui.colored_label(Color32::from_rgb(152, 195, 121), "✓ Valid XML Document");
                            ui.label(format!("({} elements, max depth: {}, verified in {:.2}s)", elements_count, max_depth, elapsed_secs));
                        }
                        XmlValidationResult::Invalid { line_number, byte_offset, message } => {
                            ui.colored_label(Color32::from_rgb(224, 108, 117), "✗ XML Validation Error");
                            ui.label(format!("Line {}, Offset {}: {}", line_number, byte_offset, message));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            dismiss_validation = true;
                        }
                    });
                });
            });
        }
        if dismiss_validation {
            self.xml_validation_result = None;
        }

        // JSON Validation Banner if available
        let mut dismiss_json_validation = false;
        if let Some(ref val_res) = self.json_validation_result {
            egui::TopBottomPanel::top("json_validation_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match val_res {
                        JsonValidationResult::Valid { objects_count, arrays_count, max_depth, elapsed_secs } => {
                            ui.colored_label(Color32::from_rgb(152, 195, 121), "✓ Valid JSON Document");
                            ui.label(format!("({} objects, {} arrays, max depth: {}, verified in {:.2}s)", objects_count, arrays_count, max_depth, elapsed_secs));
                        }
                        JsonValidationResult::Invalid { line_number, byte_offset, message } => {
                            ui.colored_label(Color32::from_rgb(224, 108, 117), "✗ JSON Validation Error");
                            ui.label(format!("Line {}, Offset {}: {}", line_number, byte_offset, message));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            dismiss_json_validation = true;
                        }
                    });
                });
            });
        }
        if dismiss_json_validation {
            self.json_validation_result = None;
        }

        // Search Results Bottom Panel
        if self.show_search_results && has_file {
            let matches_snapshot = self.search_matches.read().unwrap().clone();
            let mut selected_match = None;
            let mut close_panel = false;

            egui::TopBottomPanel::bottom("search_results_panel")
                .resizable(true)
                .default_height(160.0)
                .height_range(80.0..=400.0)
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31))))
                .show(ctx, |ui| {
                    render_search_results_panel(
                        ui,
                        &matches_snapshot,
                        self.search_matches_count(),
                        self.active_match_idx,
                        &mut selected_match,
                        &mut close_panel,
                    );
                });

            if let Some(idx) = selected_match {
                self.select_match(idx);
            }
            if close_panel {
                self.show_search_results = false;
            }
        }

        // Central Viewport Panel
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 34, 39)))
            .show(ctx, |ui| {
            if let Some(ref err) = self.error_message {
                ui.colored_label(Color32::from_rgb(220, 50, 50), err);
                ui.separator();
            }

            if self.engine.is_none() {
                // Empty state / Drag and drop target with deep obsidian background
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);

                        // Hero Logo
                        let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(40.0, 48.0), Sense::hover());
                        super::icons::paint_icon(ui.painter(), logo_rect, super::icons::Icon::Logo, Color32::from_rgb(0, 144, 255));
                        ui.add_space(14.0);

                        ui.label(RichText::new("UltraViewer").size(24.0).strong().color(Color32::from_rgb(240, 245, 255)));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Open Bigger. Explore Faster. Instant streaming up to 50 GB.").size(13.5).color(Color32::from_rgb(140, 155, 175)));

                        ui.add_space(24.0);

                        let btn_w = 190.0;
                        let btn_h = 36.0;
                        let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());
                        let hovered = btn_resp.hovered();
                        let bg = if hovered { Color32::from_rgb(16, 130, 228) } else { Color32::from_rgb(0, 120, 212) };
                        ui.painter().rect_filled(btn_rect, 5.0, bg);
                        let icon_box = egui::Rect::from_min_size(
                            egui::Pos2::new(btn_rect.left() + 20.0, btn_rect.top() + 9.0),
                            Vec2::splat(18.0),
                        );
                        super::icons::paint_icon(ui.painter(), icon_box, super::icons::Icon::FolderOpen, Color32::WHITE);
                        ui.painter().text(
                            egui::Pos2::new(btn_rect.left() + 48.0, btn_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            "Open File (Ctrl+O)",
                            egui::FontId::proportional(13.5),
                            Color32::WHITE,
                        );
                        if btn_resp.clicked() {
                            self.trigger_file_dialog();
                        }

                        ui.add_space(14.0);
                        ui.label(RichText::new("or drag and drop a file anywhere into this window").size(12.0).color(Color32::from_rgb(110, 120, 138)));
                    });
                });
            } else if self.csv_grid.is_enabled {
                render_csv_grid(ui, &self.viewport, self.font_size, self.csv_grid.delimiter);
            } else {
                // File Viewport: Virtualized Monospace Line Rendering + Document Scrollbar
                self.render_editor_viewport(ui);
            }
        });

        // Go to Line Dialog (Ctrl+G)
        if self.show_goto_line_dialog {
            let mut close_dialog = false;
            let mut target_to_jump = None;
            egui::Window::new("Go to Line")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Line number:");
                        let resp = ui.text_edit_singleline(&mut self.jump_line_input);
                        resp.request_focus();
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Ok(l) = self.jump_line_input.trim().replace(',', "").parse::<usize>() {
                                target_to_jump = Some(l);
                            }
                            close_dialog = true;
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked() {
                            if let Ok(l) = self.jump_line_input.trim().replace(',', "").parse::<usize>() {
                                target_to_jump = Some(l);
                            }
                            close_dialog = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                    });
                });

            if let Some(target) = target_to_jump {
                self.scroll_to_line(target);
            }
            if close_dialog {
                self.show_goto_line_dialog = false;
            }
        }

        // About Dialog
        if self.show_about_dialog {
            egui::Window::new("About UltraViewer")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading("UltraViewer v0.1.0 (Phase 7)");
                    ui.add_space(8.0);
                    ui.label("UltraViewer is designed for viewing and editing massive files");
                    ui.label("using memory-efficient, virtualized, background-processing architecture.");
                    ui.add_space(12.0);
                    ui.label("• Streaming Field Analyzer & Schema Profiler (Bounded Top-K Frequencies)");
                    ui.label("• Zero-DOM Streaming XML & JSON Formatter (Beautify & Minify)");
                    ui.label("• First-class XML & JSON Intelligence (Validation & Structure Tree)");
                    ui.label("• High-performance XML & JSON Syntax Highlighting");
                    ui.label("• Multi-threaded background search (> 3 GB/s)");
                    ui.label("• Checkpoint-based sparse line index");
                    ui.add_space(16.0);
                    if ui.button("Close").clicked() {
                        self.show_about_dialog = false;
                    }
                });
        }

        // Streaming Formatter Progress & Completion Modal
        if self.show_format_modal {
            if let Some(ref active) = self.active_formatting {
                let progress = active.progress.read().unwrap().clone();
                let modal_action = render_format_modal(
                    ctx,
                    &active.title,
                    &progress,
                    &active.dst_path,
                    active.is_in_place,
                );

                if let Some(act) = modal_action {
                    match act {
                        FormatModalAction::Cancel => {
                            self.cancel_formatting();
                            self.show_format_modal = false;
                            self.active_formatting = None;
                        }
                        FormatModalAction::Dismiss => {
                            self.show_format_modal = false;
                            self.active_formatting = None;
                        }
                        FormatModalAction::OpenTarget(path) => {
                            self.show_format_modal = false;
                            self.active_formatting = None;
                            self.open_file(path);
                        }
                    }
                } else if active.is_in_place && progress.is_finished {
                    // Automatically swap in-place once finished!
                    let src = active.src_path.clone();
                    let dst = active.dst_path.clone();
                    self.show_format_modal = false;
                    self.active_formatting = None;
                    self.close_file();
                    if let Err(e) = std::fs::rename(&dst, &src) {
                        self.error_message = Some(format!("Failed to replace formatted file: {}", e));
                    } else {
                        self.open_file(src);
                    }
                }
            }
        }

        // Field Analyzer & Schema Profiler Panel
        if let Some(act) = render_analyzer_panel(ctx, &mut self.analyzer_state) {
            match act {
                AnalyzerPanelAction::Dismiss => {
                    self.cancel_analysis();
                    self.analyzer_state.is_open = false;
                }
                AnalyzerPanelAction::Cancel => {
                    self.cancel_analysis();
                    self.analyzer_state.set_cancelled();
                }
                AnalyzerPanelAction::SearchValue(val) => {
                    self.search_query.pattern = val;
                    self.search_query.case_sensitive = true;
                    self.search_query.is_regex = false;
                    self.show_search_bar = true;
                    self.show_search_results = true;
                    self.start_search();
                }
                AnalyzerPanelAction::ExportJson => {
                    self.export_analysis_schema();
                }
            }
        }
        if self.show_save_modal {
            if let Some(ref active) = self.active_saving {
                let progress = active.progress.read().unwrap().clone();
                let modal_action = render_format_modal(
                    ctx,
                    &active.title,
                    &progress,
                    &active.target_path,
                    active.is_in_place,
                );

                if let Some(act) = modal_action {
                    match act {
                        FormatModalAction::Cancel => {
                            self.cancel_saving();
                            self.show_save_modal = false;
                        }
                        FormatModalAction::Dismiss => {
                            self.show_save_modal = false;
                        }
                        FormatModalAction::OpenTarget(path) => {
                            self.show_save_modal = false;
                            self.open_file(path);
                        }
                    }
                }
            }
        }

        // Full Line Inspector Window
        if self.show_full_line_inspector {
            let mut close_inspector = false;
            if let Some(ref inspector) = self.full_line_inspector {
                let title = format!("🔍 Full Line Inspector — Line {}", inspector.line_number);
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(true)
                    .default_size([760.0, 420.0])
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Line {}  ·  {} chars",
                                    inspector.line_number,
                                    format_number(inspector.byte_len as u64)
                                ))
                                .weak(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("✖ Close").clicked() {
                                    close_inspector = true;
                                }
                                if ui.button("📋 Copy").clicked() {
                                    ui.ctx().copy_text(inspector.full_text.clone());
                                }
                            });
                        });
                        ui.separator();
                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(
                                        &mut inspector.full_text.as_str()
                                    )
                                    .font(egui::FontId::monospace(13.0))
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(16)
                                    .interactive(false),
                                );
                            });
                    });
            }
            if close_inspector {
                self.show_full_line_inspector = false;
            }
        }

        // Unsaved Changes Confirmation Dialog
        if self.show_unsaved_dialog {
            let mut close_dialog = false;
            let mut user_choice = None; // None: still open, Some(true): save, Some(false): discard
            let mut cancel_pending = false;

            egui::Window::new(RichText::new("⚠️ Unsaved Changes").strong())
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label("This document has unsaved modifications.");
                    ui.label("Do you want to save your changes before proceeding?");
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if ui.button("💾 Save").clicked() {
                            user_choice = Some(true);
                            close_dialog = true;
                        }
                        if ui.button("🗑 Discard").clicked() {
                            user_choice = Some(false);
                            close_dialog = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_pending = true;
                            close_dialog = true;
                        }
                    });
                });

            if close_dialog {
                self.show_unsaved_dialog = false;
                if let Some(should_save) = user_choice {
                    if should_save {
                        self.start_save_in_place();
                    } else {
                        // Discard and proceed
                        if let Some(pending) = self.pending_file_action.take() {
                            match pending {
                                PendingFileAction::Open(p) => self.do_open_file(&p),
                                PendingFileAction::Close => self.do_close_file(),
                                PendingFileAction::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                            }
                        }
                    }
                } else if cancel_pending {
                    self.pending_file_action = None;
                }
            }
        }

        // Command Palette Modal (Ctrl+Shift+P)
        if let Some(pal_action) = render_command_palette(ctx, &mut self.command_palette) {
            self.handle_palette_action(pal_action, ctx);
        }

        // Side-by-Side Diff Modal
        if self.diff_viewer.is_open {
            let primary_lines: Vec<String> = self.viewport.lines.iter().map(|l| l.text.clone()).collect();
            let primary_name = self.engine.as_ref()
                .and_then(|e| e.path().file_name().and_then(|n| n.to_str()))
                .unwrap_or("Active File");

            if let Some(act) = render_diff_modal(ctx, &mut self.diff_viewer, &primary_lines, primary_name) {
                match act {
                    DiffViewerAction::Close => self.diff_viewer.is_open = false,
                    DiffViewerAction::PickSecondaryFile => {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                self.diff_viewer.secondary_lines = content.lines().map(|s| s.to_string()).collect();
                                self.diff_viewer.secondary_path = Some(path);
                            }
                        }
                    }
                }
            }
        }

        // Temporary Status Notification Toast
        if let Some((ref msg, created_at)) = self.status_notification {
            if created_at.elapsed() < Duration::from_secs(4) {
                egui::Window::new("NotificationToast")
                    .title_bar(false)
                    .resizable(false)
                    .collapsible(false)
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -36.0))
                    .frame(
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(30, 34, 39))
                            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 122, 204)))
                            .inner_margin(egui::Margin::symmetric(12, 8)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("ℹ").color(Color32::from_rgb(97, 175, 239)).strong());
                            ui.label(RichText::new(msg).size(12.0).color(Color32::WHITE));
                        });
                    });
            } else {
                self.status_notification = None;
            }
        }
    }
}

impl UltraViewerApp {
    fn handle_palette_action(&mut self, action: PaletteAction, ctx: &egui::Context) {
        match action {
            PaletteAction::OpenFile => self.trigger_file_dialog(),
            PaletteAction::OpenFolder => self.trigger_folder_dialog(),
            PaletteAction::CloseActiveTab => self.close_file(),
            PaletteAction::CloseAllTabs => self.close_all_tabs(),
            PaletteAction::SaveFile => self.start_save_in_place(),
            PaletteAction::SaveFileAs => self.start_save_as(),
            PaletteAction::Find => {
                self.show_search_bar = true;
                self.focus_search_input = true;
            }
            PaletteAction::GoToLine => self.show_goto_line_dialog = true,
            PaletteAction::ToggleEditMode => {
                self.is_edit_mode = !self.is_edit_mode;
                if !self.is_edit_mode {
                    self.active_edit_line = None;
                }
            }
            PaletteAction::ToggleWrap => {
                self.word_wrap = !self.word_wrap;
                self.persist_session();
            }
            PaletteAction::ZoomIn => {
                self.font_size = (self.font_size + 1.0).min(36.0);
                self.persist_session();
            }
            PaletteAction::ZoomOut => {
                self.font_size = (self.font_size - 1.0).max(8.0);
                self.persist_session();
            }
            PaletteAction::ZoomReset => {
                self.font_size = 14.0;
                self.persist_session();
            }
            PaletteAction::SetThemeOneDark => self.set_theme(ColorTheme::OneDarkProDarker, ctx),
            PaletteAction::SetThemeGitHubDark => self.set_theme(ColorTheme::GitHubDark, ctx),
            PaletteAction::SetThemeMonokai => self.set_theme(ColorTheme::MonokaiPro, ctx),
            PaletteAction::SetThemeTokyoNight => self.set_theme(ColorTheme::TokyoNight, ctx),
            PaletteAction::SetThemeLightModern => self.set_theme(ColorTheme::LightModern, ctx),
            PaletteAction::XmlValidate => self.validate_xml_document(),
            PaletteAction::XmlToggleTree => {
                self.show_xml_tree = !self.show_xml_tree;
                if self.show_xml_tree && self.xml_tree_root.is_none() {
                    self.trigger_build_xml_tree();
                }
            }
            PaletteAction::JsonValidate => self.validate_json_document(),
            PaletteAction::JsonToggleTree => {
                self.show_json_tree = !self.show_json_tree;
                if self.show_json_tree && self.json_tree_root.is_none() {
                    self.trigger_build_json_tree();
                }
            }
            PaletteAction::FormatBeautify2 => {
                self.start_formatting(FormatAction::Beautify { indent_size: 2, use_tabs: false }, true, None);
            }
            PaletteAction::FormatBeautify4 => {
                self.start_formatting(FormatAction::Beautify { indent_size: 4, use_tabs: false }, true, None);
            }
            PaletteAction::FormatMinify => {
                self.start_formatting(FormatAction::Minify, true, None);
            }
            PaletteAction::AnalyzeFields => self.start_analysis(),
            PaletteAction::RegisterContextMenu => {
                match ContextMenuManager::register() {
                    Ok(msg) => self.status_notification = Some((msg, Instant::now())),
                    Err(err) => self.error_message = Some(err),
                }
            }
            PaletteAction::UnregisterContextMenu => {
                match ContextMenuManager::unregister() {
                    Ok(msg) => self.status_notification = Some((msg, Instant::now())),
                    Err(err) => self.error_message = Some(err),
                }
            }
            PaletteAction::About => self.show_about_dialog = true,
        }
    }
    fn render_editor_viewport(&mut self, ui: &mut Ui) {
        let text_font = FontId::monospace(self.font_size);
        let line_num_font = FontId::monospace((self.font_size * 0.9).max(10.0));
        let line_num_color = Color32::from_rgb(92, 99, 112); // #5C6370 VS Code line number color

        let total_lines = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(1).max(1);
        let file_type = self.file_type;
        let enable_highlighting = self.enable_syntax_highlighting;

        let active_match_line = self.active_match_idx.and_then(|idx| {
            self.search_matches.read().unwrap().get(idx).map(|m| m.line_number)
        });
        let search_pattern = if self.show_search_bar && !self.search_query.pattern.is_empty() {
            Some(self.search_query.pattern.clone())
        } else {
            None
        };
        let search_case = self.search_query.case_sensitive;
        let word_wrap = self.word_wrap;
        let mut scroll_target = None;
        // Inspector open request deferred outside UI closures to avoid borrow conflicts.
        let mut open_inspector_for: Option<(usize, u64)> = None;

        let dark_mode = ui.visuals().dark_mode;
        let is_edit_mode = self.is_edit_mode;
        let active_edit = self.active_edit_line;

        let mut commit_edit = None;
        let mut cancel_edit = false;
        let mut start_edit_for = None;

        ui.horizontal(|ui| {
            let slider_width = 18.0;
            let available_w = ui.available_width();
            let viewport_w = (available_w - slider_width - 4.0).max(100.0);

            // Document Viewport
            ui.allocate_ui_with_layout(
                egui::vec2(viewport_w, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    let gutter_w = 65.0;

                    if word_wrap {
                        // Word Wrap ON: Virtualized vertical line window with per-line horizontal rows
                        let wrap_width = (ui.available_width() - gutter_w - 20.0).max(100.0);
                        let top_left = ui.cursor().min;

                        ui.vertical(|ui| {
                            for line in &self.viewport.lines {
                                let line_no = line.line_number;
                                let is_active_search = active_match_line == Some(line_no);
                                let is_editing_this_line = is_edit_mode && active_edit == Some(line_no);
                                let is_truncated = line.is_truncated;
                                let line_byte_offset = line.byte_offset;

                                ui.horizontal_top(|ui| {
                                    // Line number gutter (fixed width, top-aligned)
                                    let num_str = if is_active_search {
                                        format!("▶{:>6} ", format_number(line_no as u64))
                                    } else {
                                        format!("{:>7} ", format_number(line_no as u64))
                                    };
                                    let color = if is_active_search {
                                        Color32::from_rgb(255, 215, 0)
                                    } else {
                                        line_num_color
                                    };

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(gutter_w, 0.0),
                                        egui::Layout::right_to_left(egui::Align::Min),
                                        |ui| {
                                            ui.label(RichText::new(num_str).font(line_num_font.clone()).color(color));
                                        },
                                    );

                                    ui.add_space(8.0);

                                    // Line content (wrapped)
                                    if is_editing_this_line {
                                        let resp = ui.add(
                                            egui::TextEdit::singleline(&mut self.edit_line_buffer)
                                                .font(text_font.clone())
                                                .desired_width(f32::INFINITY)
                                        );
                                        resp.request_focus();

                                        if resp.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                            commit_edit = Some((line_no, line_byte_offset, line.text.clone()));
                                        } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                                            cancel_edit = true;
                                        }
                                    } else {
                                        let job = build_line_layout_job(
                                            &line.text,
                                            file_type,
                                            enable_highlighting,
                                            search_pattern.as_deref(),
                                            search_case,
                                            is_active_search,
                                            text_font.clone(),
                                            dark_mode,
                                            wrap_width,
                                        );

                                        let label = egui::Label::new(job)
                                            .wrap_mode(egui::TextWrapMode::Wrap)
                                            .sense(if is_edit_mode {
                                                egui::Sense::click()
                                            } else {
                                                egui::Sense::hover()
                                            });
                                        let label_resp = ui.add(label);

                                        if is_edit_mode && label_resp.clicked() {
                                            start_edit_for = Some((line_no, line.text.clone()));
                                        }

                                        if is_truncated {
                                            let btn = egui::Button::new(
                                                egui::RichText::new("↗ full line")
                                                    .small()
                                                    .color(egui::Color32::from_rgb(100, 160, 255))
                                            )
                                            .frame(false);
                                            if ui.add(btn).on_hover_text("Click to open Full Line Inspector").clicked() {
                                                open_inspector_for = Some((line_no, line_byte_offset));
                                            }
                                        }
                                    }
                                });
                            }
                        });

                        // Draw clean vertical separator line between gutter and text
                        let gutter_divider_x = top_left.x + gutter_w + 4.0;
                        let bottom_y = ui.cursor().min.y;
                        ui.painter().vline(
                            gutter_divider_x,
                            top_left.y..=bottom_y,
                            ui.visuals().widgets.noninteractive.bg_stroke,
                        );
                    } else {
                        // Word Wrap OFF: Horizontal scrolling with pinned line numbers column
                        ScrollArea::horizontal()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Line numbers column
                                    ui.vertical(|ui| {
                                        ui.set_min_width(65.0);
                                        for line in &self.viewport.lines {
                                            let is_active = active_match_line == Some(line.line_number);
                                            let num_str = if is_active {
                                                format!("▶{:>6} ", format_number(line.line_number as u64))
                                            } else {
                                                format!("{:>7} ", format_number(line.line_number as u64))
                                            };
                                            let color = if is_active {
                                                Color32::from_rgb(255, 215, 0)
                                            } else {
                                                line_num_color
                                            };
                                            ui.label(RichText::new(num_str).font(line_num_font.clone()).color(color));
                                        }
                                    });

                                    ui.separator();

                                    // Content column
                                    ui.vertical(|ui| {
                                        for line in &self.viewport.lines {
                                            let line_no = line.line_number;
                                            let is_active_search = active_match_line == Some(line_no);
                                            let is_editing_this_line = is_edit_mode && active_edit == Some(line_no);
                                            let is_truncated = line.is_truncated;
                                            let line_byte_offset = line.byte_offset;

                                            if is_editing_this_line {
                                                let resp = ui.add(
                                                    egui::TextEdit::singleline(&mut self.edit_line_buffer)
                                                        .font(text_font.clone())
                                                        .desired_width(f32::INFINITY)
                                                );
                                                resp.request_focus();

                                                if resp.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                    commit_edit = Some((line_no, line.byte_offset, line.text.clone()));
                                                } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                                                    cancel_edit = true;
                                                }
                                            } else {
                                                ui.horizontal(|ui| {
                                                    let job = build_line_layout_job(
                                                        &line.text,
                                                        file_type,
                                                        enable_highlighting,
                                                        search_pattern.as_deref(),
                                                        search_case,
                                                        is_active_search,
                                                        text_font.clone(),
                                                        dark_mode,
                                                        f32::INFINITY,
                                                    );

                                                    let label_resp = ui.add(
                                                        egui::Label::new(job)
                                                            .wrap_mode(egui::TextWrapMode::Extend)
                                                            .sense(if is_edit_mode {
                                                                egui::Sense::click()
                                                            } else {
                                                                egui::Sense::hover()
                                                            })
                                                    );

                                                    if is_edit_mode && label_resp.clicked() {
                                                        start_edit_for = Some((line_no, line.text.clone()));
                                                    }

                                                    if is_truncated {
                                                        let btn = egui::Button::new(
                                                            egui::RichText::new("↗ full line")
                                                                .small()
                                                                .color(egui::Color32::from_rgb(100, 160, 255))
                                                        )
                                                        .frame(false);
                                                        if ui.add(btn).on_hover_text("Click to open Full Line Inspector").clicked() {
                                                            open_inspector_for = Some((line_no, line_byte_offset));
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    });
                                });
                            });
                    }
                },
            );

            // Heatmap Overview Ruler on the right (Option B)
            let matches_guard = self.search_matches.read().unwrap();
            let ruler_props = OverviewRulerProps {
                current_line: self.current_line,
                visible_lines_count: self.viewport.lines.len(),
                total_lines,
                search_matches: &matches_guard,
                active_match_line,
            };
            let (target_line, _) = render_overview_ruler(ui, ui.available_height(), &ruler_props);
            if let Some(target) = target_line {
                scroll_target = Some(target);
            }
        });

        if let Some((line_no, byte_offset, old_text)) = commit_edit {
            self.commit_line_edit(line_no, byte_offset, &old_text);
        }
        if cancel_edit {
            self.active_edit_line = None;
        }
        if let Some((line_no, text)) = start_edit_for {
            self.active_edit_line = Some(line_no);
            self.edit_line_buffer = text;
        }

        if let Some(target) = scroll_target {
            self.scroll_to_line(target);
        }

        // Handle inspector open request — must be done after the UI borrows above are released.
        if let Some((line_no, byte_offset)) = open_inspector_for {
            if let Some(ref engine) = self.engine {
                // Compute byte length from adjacent viewport line offsets.
                let next_offset = self.viewport.lines
                    .iter()
                    .find(|l| l.byte_offset > byte_offset)
                    .map(|l| l.byte_offset)
                    .unwrap_or_else(|| engine.size());

                let raw_len = (next_offset.saturating_sub(byte_offset)) as usize;
                let read_len = raw_len.min(4 * 1024 * 1024); // safety cap: 4 MB

                let full_text = match engine.read_range(byte_offset, read_len) {
                    Ok(bytes) => {
                        // Strip trailing \r\n / \n
                        let s = String::from_utf8_lossy(bytes);
                        s.trim_end_matches('\n').trim_end_matches('\r').to_string()
                    }
                    Err(e) => format!("[Error reading line: {e}]"),
                };

                self.full_line_inspector = Some(FullLineInspector {
                    line_number: line_no,
                    byte_len: full_text.len(),
                    full_text,
                });
                self.show_full_line_inspector = true;
            }
        }
    }
}

fn build_line_layout_job(
    line: &str,
    file_type: Option<FileType>,
    enable_highlighting: bool,
    search_pattern: Option<&str>,
    case_sensitive: bool,
    is_active_line: bool,
    font: FontId,
    dark_mode: bool,
    wrap_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    // INFINITY = no wrap (default); finite = wrap at pixel boundary
    job.wrap.max_width = wrap_width;

    let default_text_color = if dark_mode {
        Color32::from_rgb(171, 178, 191) // #ABB2BF One Dark foreground
    } else {
        Color32::from_rgb(30, 30, 30)
    };

    let spans: Vec<(String, Color32)> = if enable_highlighting {
        match file_type {
            Some(FileType::Xml) => XmlSyntaxHighlighter::highlight_line_themed(line, dark_mode)
                .into_iter()
                .map(|s| (s.text, s.color))
                .collect(),
            Some(FileType::Json) => JsonSyntaxHighlighter::highlight_line_themed(line, dark_mode)
                .into_iter()
                .map(|s| (s.text, s.color))
                .collect(),
            _ => vec![(line.to_string(), default_text_color)],
        }
    } else {
        vec![(line.to_string(), default_text_color)]
    };

    let active_bg = Color32::from_rgb(255, 230, 0); // Bright gold highlight
    let active_fg = Color32::from_rgb(15, 15, 15);   // Dark text
    let inactive_bg = if dark_mode {
        Color32::from_rgb(120, 100, 20) // Muted amber highlight
    } else {
        Color32::from_rgb(255, 235, 150) // Soft yellow highlight
    };
    let inactive_fg = if dark_mode {
        Color32::from_rgb(255, 255, 230)
    } else {
        Color32::from_rgb(20, 20, 20)
    };

    for (span_text, span_color) in spans {
        if let Some(pattern) = search_pattern {
            if !pattern.is_empty() {
                let match_indices: Vec<(usize, usize)> = if case_sensitive {
                    span_text.match_indices(pattern).map(|(i, s)| (i, i + s.len())).collect()
                } else {
                    let text_lower = span_text.to_lowercase();
                    let pat_lower = pattern.to_lowercase();
                    let pat_len = pat_lower.len();
                    let mut idxs = Vec::new();
                    let mut search_from = 0;
                    while let Some(pos) = text_lower[search_from..].find(&pat_lower) {
                        let abs_start = search_from + pos;
                        let abs_end = abs_start + pat_len;
                        if span_text.is_char_boundary(abs_start) && span_text.is_char_boundary(abs_end) {
                            idxs.push((abs_start, abs_end));
                        }
                        search_from = abs_start + pat_len.max(1);
                    }
                    idxs
                };

                if !match_indices.is_empty() {
                    let mut start_idx = 0;
                    for (m_start, m_end) in match_indices {
                        if m_start > start_idx {
                            job.append(
                                &span_text[start_idx..m_start],
                                0.0,
                                TextFormat {
                                    font_id: font.clone(),
                                    color: span_color,
                                    ..Default::default()
                                },
                            );
                        }

                        let (bg, fg) = if is_active_line {
                            (active_bg, active_fg)
                        } else {
                            (inactive_bg, inactive_fg)
                        };

                        job.append(
                            &span_text[m_start..m_end],
                            0.0,
                            TextFormat {
                                font_id: font.clone(),
                                color: fg,
                                background: bg,
                                ..Default::default()
                            },
                        );

                        start_idx = m_end;
                    }

                    if start_idx < span_text.len() {
                        job.append(
                            &span_text[start_idx..],
                            0.0,
                            TextFormat {
                                font_id: font.clone(),
                                color: span_color,
                                ..Default::default()
                            },
                        );
                    }
                    continue;
                }
            }
        }

        job.append(
            &span_text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: span_color,
                ..Default::default()
            },
        );
    }

    job
}
