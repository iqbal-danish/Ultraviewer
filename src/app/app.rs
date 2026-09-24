use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Color32, FontId, Key, Pos2, Rect, RichText, ScrollArea, Sense, TextFormat, Ui, Vec2};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::editor::{EditorDocument, PieceTableReader, SaveManager, Viewport, ViewportLine};
use crate::file_engine::{FileEngine, LineIndex, SliceSource, VirtualSlice};
use crate::formats::formatter::{FormatAction, FormattingProgress, JsonStreamingFormatter, XmlStreamingFormatter};
use crate::formats::json::{JsonStructureIndexer, JsonSyntaxHighlighter, JsonTreeNode, JsonValidationResult, JsonValidator};
use crate::formats::xml::{XmlStructureIndexer, XmlSyntaxHighlighter, XmlTreeNode, XmlValidationResult, XmlValidator};
use crate::analysis::field_analyzer::{AnalysisReport, StreamingFieldAnalyzer};
use crate::analysis::field_extractor::StreamingFieldExtractor;
use crate::formats::path_resolver::PathResolver;
use crate::formats::{FileType, FormatDetector, JsonPathQuery, QueryMatch, StreamingQueryEngine, XPathQuery};
use crate::indexing::LineIndexer;
use crate::search::{SearchQuery, SearchResultMatch, SearchStatus, SearchWorker};
use super::activity_bar::{render_activity_bar, ActivityBarAction, ActivityBarProps, ActivityPanel};
use super::analyzer_panel::{render_analyzer_panel, AnalyzerPanelAction, AnalyzerPanelState};
use super::breadcrumb_bar::{render_breadcrumb_bar, BreadcrumbAction, BreadcrumbBarProps};
use super::command_palette::{render_command_palette, CommandPaletteState, PaletteAction};
use super::context_menu::ContextMenuManager;
use super::csv_grid::{render_csv_grid, split_delimited, CsvGridState};
use super::diff_viewer::{render_diff_modal, DiffViewerAction, DiffViewerState};
use super::field_extract_modal::{render_field_extract_modal, FieldExtractModalAction, FieldExtractModalState};
use super::folder_explorer::{render_folder_explorer, FolderAction, FolderExplorerState};
use super::format_modal::{render_format_modal, render_format_options_modal, FormatModalAction, FormatOptionsModalAction, FormatOptionsModalState, FormatTarget};
use super::icons::{paint_icon, Icon};
use super::json_tree_panel::render_json_tree_panel;
use super::menu::{render_menu_bar, MenuAction};
use super::overview_ruler::{render_overview_ruler, OverviewRulerProps};
use super::search_panel::{render_search_bar, render_search_results_panel, SearchBarAction};
use super::session::AppSession;
use super::status_bar::{format_number, render_status_bar, StatusBarAction, StatusBarProps};
use super::tab_bar::{render_tab_bar, TabBarAction, TabBarProps, TabInfo};
use super::url_modal::{render_url_modal, UrlModalAction, UrlModalState};
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
    pub folded_lines: HashSet<usize>,
    pub selection_anchor: Option<usize>,
    pub selection_head: Option<usize>,
    pub current_xpath: Option<String>,
}

pub struct UltraViewerApp {
    pub engine: Option<Arc<FileEngine>>,
    pub line_index: Option<Arc<LineIndex>>,
    pub indexer_cancel: Option<Arc<AtomicBool>>,
    pub indexer_handle: Option<JoinHandle<()>>,

    pub viewport: Viewport,
    pub file_type: Option<FileType>,
    pub open_duration: Option<Duration>,
    pub font_size: f32,

    pub current_line: usize,
    pub jump_line_input: String,
    pub jump_offset_input: String,

    // Search & Replace state
    pub show_search_bar: bool,
    pub show_search_results: bool,
    pub focus_search_input: bool,
    pub show_replace_bar: bool,
    pub replace_text: String,
    pub focus_replace_input: bool,
    pub search_query: SearchQuery,
    pub last_executed_query: Option<SearchQuery>,
    pub auto_jump_to_first_match: bool,
    pub search_status: Arc<RwLock<SearchStatus>>,
    pub search_matches: Arc<RwLock<Vec<SearchResultMatch>>>,
    pub search_cancel: Option<Arc<AtomicBool>>,
    pub search_handle: Option<JoinHandle<()>>,
    pub active_match_idx: Option<usize>,

    // XML Intelligence state
    pub enable_syntax_highlighting: bool,
    pub word_wrap: bool,
    pub show_xml_tree: bool,
    pub xml_tree_root: Option<Arc<XmlTreeNode>>,
    pub xml_tree_building: bool,
    pub xml_tree_rx: Option<crossbeam_channel::Receiver<Option<XmlTreeNode>>>,
    pub xml_tree_cancel: Option<Arc<AtomicBool>>,
    pub xml_validation_result: Option<XmlValidationResult>,
    pub is_validating_xml: bool,
    pub xml_validation_rx: Option<crossbeam_channel::Receiver<XmlValidationResult>>,
    pub xml_validation_cancel: Option<Arc<AtomicBool>>,

    // JSON Intelligence state
    pub show_json_tree: bool,
    pub json_tree_root: Option<Arc<JsonTreeNode>>,
    pub json_tree_building: bool,
    pub json_tree_rx: Option<crossbeam_channel::Receiver<Arc<JsonTreeNode>>>,
    pub json_tree_cancel: Option<Arc<AtomicBool>>,
    pub json_validation_result: Option<JsonValidationResult>,
    pub is_validating_json: bool,
    pub json_validation_rx: Option<crossbeam_channel::Receiver<JsonValidationResult>>,
    pub json_validation_cancel: Option<Arc<AtomicBool>>,

    pub show_about_dialog: bool,
    pub show_goto_line_dialog: bool,
    pub active_formatting: Option<ActiveFormatting>,
    pub show_format_modal: bool,
    pub analyzer_state: AnalyzerPanelState,
    pub active_analysis: Option<ActiveAnalysis>,
    pub error_message: Option<String>,

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

    // Code folding & Multi-line Selection
    pub folded_lines: HashSet<usize>,
    pub selection_anchor: Option<usize>,
    pub selection_anchor_col: Option<usize>,
    pub selection_head: Option<usize>,
    pub selection_head_col: Option<usize>,
    pub gutter_drag_active: bool,
    pub mouse_drag_start_line: Option<usize>,
    pub mouse_drag_start_col: Option<usize>,
    pub pending_focus_line: Option<(usize, usize)>,
    pub editor_viewport_rect: Option<egui::Rect>,
    pub cached_screen_lines: usize,
    pub untitled_counter: usize,
    pub line_edit_initial_texts: std::collections::HashMap<usize, String>,

    // XPath / JSONPath resolution state
    pub current_xpath: Option<String>,
    pub last_resolved_line: usize,
    pub last_resolved_col: Option<usize>,
    pub pending_xpath: Option<(usize, Option<usize>, Instant)>,
    pub xpath_rx: Option<crossbeam_channel::Receiver<(u64, String)>>,
    pub xpath_tx: Option<crossbeam_channel::Sender<(u64, String)>>,
    pub xpath_generation: Arc<AtomicU64>,

    // Advanced Streaming Features State
    pub show_query_bar: bool,
    pub query_text: String,
    pub query_matches: Vec<QueryMatch>,
    pub query_match_idx: usize,
    pub is_query_running: bool,
    pub query_cancel: Option<Arc<AtomicBool>>,
    pub query_rx: Option<crossbeam_channel::Receiver<Vec<QueryMatch>>>,
    pub query_generation: Arc<AtomicU64>,
    pub discovered_tags: crate::formats::DiscoveredTags,
    pub show_query_suggestions: bool,

    pub active_slice: Option<VirtualSlice>,
    pub field_extract_state: FieldExtractModalState,
    pub field_extract_cancel: Option<Arc<AtomicBool>>,
    pub field_extract_rx: Option<crossbeam_channel::Receiver<(PathBuf, bool, Result<u64, String>)>>,
    pub field_extract_prog_rx: Option<crossbeam_channel::Receiver<crate::analysis::ExtractionProgress>>,
    pub format_options_state: FormatOptionsModalState,
    pub url_modal_state: UrlModalState,
    pub auto_save: bool,
    pub auto_save_timer: Option<Instant>,
    pub pending_slice_on_query_complete: bool,
    pub scroll_accumulator: f32,

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
        let auto_save_init = session.auto_save;

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
            show_replace_bar: false,
            replace_text: String::new(),
            focus_replace_input: false,
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
            xml_validation_rx: None,
            xml_validation_cancel: None,

            show_json_tree: false,
            json_tree_root: None,
            json_tree_building: false,
            json_tree_rx: None,
            json_tree_cancel: None,
            json_validation_result: None,
            is_validating_json: false,
            json_validation_rx: None,
            json_validation_cancel: None,

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

            folded_lines: HashSet::new(),
            selection_anchor: None,
            selection_anchor_col: None,
            selection_head: None,
            selection_head_col: None,
            gutter_drag_active: false,
            mouse_drag_start_line: None,
            mouse_drag_start_col: None,
            pending_focus_line: None,
            editor_viewport_rect: None,
            cached_screen_lines: 30,
            untitled_counter: 0,
            line_edit_initial_texts: std::collections::HashMap::new(),
            current_xpath: None,
            last_resolved_line: 0,
            last_resolved_col: None,
            pending_xpath: None,
            xpath_rx: None,
            xpath_tx: None,
            xpath_generation: Arc::new(AtomicU64::new(0)),

            show_query_bar: false,
            query_text: String::new(),
            query_matches: Vec::new(),
            query_match_idx: 0,
            is_query_running: false,
            query_cancel: None,
            query_rx: None,
            query_generation: Arc::new(AtomicU64::new(0)),
            discovered_tags: crate::formats::DiscoveredTags::default(),
            show_query_suggestions: false,

            active_slice: None,
            field_extract_state: FieldExtractModalState::default(),
            field_extract_cancel: None,
            field_extract_rx: None,
            field_extract_prog_rx: None,
            format_options_state: FormatOptionsModalState::default(),
            url_modal_state: UrlModalState::default(),
            auto_save: auto_save_init,
            auto_save_timer: None,
            pending_slice_on_query_complete: false,
            scroll_accumulator: 0.0,

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

    pub fn new_blank_file(&mut self) {
        self.untitled_counter += 1;
        let temp_dir = std::env::temp_dir().join("UltraViewer");
        if !temp_dir.exists() {
            let _ = std::fs::create_dir_all(&temp_dir);
        }
        let blank_path = temp_dir.join(format!("Untitled-{}.txt", self.untitled_counter));
        let _ = std::fs::File::create(&blank_path);
        self.do_open_file(&blank_path);
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        match (self.selection_anchor, self.selection_head) {
            (Some(a), Some(h)) => Some((a.min(h), a.max(h))),
            (Some(a), None) => Some((a, a)),
            (None, Some(h)) => Some((h, h)),
            (None, None) => None,
        }
    }

    pub fn char_selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        match (self.selection_anchor, self.selection_anchor_col, self.selection_head, self.selection_head_col) {
            (Some(al), Some(ac), Some(hl), Some(hc)) => {
                if (al, ac) <= (hl, hc) {
                    Some(((al, ac), (hl, hc)))
                } else {
                    Some(((hl, hc), (al, ac)))
                }
            }
            (Some(al), _, Some(hl), _) => {
                let (min_l, max_l) = if al <= hl { (al, hl) } else { (hl, al) };
                Some(((min_l, 0), (max_l, usize::MAX)))
            }
            (Some(al), Some(ac), None, _) => Some(((al, ac), (al, ac))),
            _ => None,
        }
    }

    pub fn line_selection_range(&self, line_no: usize, line_text: &str) -> Option<(usize, usize)> {
        compute_line_selection(self.char_selection_range(), line_no, line_text)
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let ((start_line, start_col), (end_line, end_col)) = self.char_selection_range()?;
        if start_line == end_line && start_col == end_col {
            return None;
        }
        let mut selected_text = Vec::new();
        for line in &self.viewport.lines {
            if line.line_number >= start_line && line.line_number <= end_line {
                let char_count = line.text.chars().count();
                let char_to_byte = |char_idx: usize| -> usize {
                    if char_idx == 0 {
                        0
                    } else if char_idx >= char_count {
                        line.text.len()
                    } else {
                        line.text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(line.text.len())
                    }
                };
                let sel = if start_line == end_line {
                    let b_start = char_to_byte(start_col);
                    let b_end = char_to_byte(end_col);
                    if b_start < b_end { Some((b_start, b_end)) } else { None }
                } else if line.line_number == start_line {
                    let b_start = char_to_byte(start_col);
                    if b_start < line.text.len() { Some((b_start, line.text.len())) } else { None }
                } else if line.line_number == end_line {
                    let b_end = char_to_byte(end_col);
                    if b_end > 0 { Some((0, b_end)) } else { None }
                } else {
                    Some((0, line.text.len()))
                };

                if let Some((b_start, b_end)) = sel {
                    if b_start <= b_end && b_end <= line.text.len() {
                        selected_text.push(line.text[b_start..b_end].to_string());
                    }
                }
            }
        }
        if selected_text.is_empty() {
            None
        } else {
            Some(selected_text.join("\n"))
        }
    }

    pub fn copy_selection(&self, ctx: &egui::Context) {
        if let Some(text) = self.get_selected_text() {
            ctx.copy_text(text);
        }
    }

    pub fn cut_selection_to_string(&mut self) -> Option<String> {
        let text = self.get_selected_text();
        self.delete_selection();
        text
    }

    pub fn cut_selection(&mut self, ctx: &egui::Context) {
        if let Some(text) = self.cut_selection_to_string() {
            ctx.copy_text(text);
        }
    }

    /// Authoritative total line count for the document, accurately reflecting
    /// background indexing as well as any in-memory line deletions, splits, or merges.
    pub fn get_total_lines(&self) -> usize {
        let base = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(1);
        let delta = self.document.as_ref().map_or(0, |d| d.total_lines_delta);
        ((base as i64 + delta).max(1)) as usize
    }

    /// Scans the piece table's virtual document to find the byte offsets where
    /// two 1-based line numbers begin. Performs a single pass over the content.
    /// This is the authoritative source of truth when the piece table has been
    /// modified by prior edits and viewport / line-index offsets may be stale.
    fn piece_table_line_offsets_pair(
        piece_table: &crate::editor::piece_table::PieceTable,
        engine: &crate::file_engine::FileEngine,
        line_a: usize,
        line_b: usize,
    ) -> (Option<u64>, Option<u64>) {
        let mut res_a = if line_a <= 1 { Some(0u64) } else { None };
        let mut res_b = if line_b <= 1 { Some(0u64) } else { None };
        if res_a.is_some() && res_b.is_some() {
            return (res_a, res_b);
        }
        let total_len = piece_table.total_length();
        let chunk_size: usize = 65536;
        let mut current_line: usize = 1;
        let mut read_pos: u64 = 0;
        let max_target = line_a.max(line_b);
        while read_pos < total_len && current_line < max_target {
            let to_read = ((total_len - read_pos) as usize).min(chunk_size);
            let mut buf = Vec::with_capacity(to_read);
            if piece_table.read_range(engine, read_pos, to_read, &mut buf).is_err() {
                break;
            }
            for (i, &b) in buf.iter().enumerate() {
                if b == b'\n' {
                    current_line += 1;
                    let off = read_pos + i as u64 + 1;
                    if current_line == line_a && res_a.is_none() {
                        res_a = Some(off);
                    }
                    if current_line == line_b && res_b.is_none() {
                        res_b = Some(off);
                    }
                    if res_a.is_some() && res_b.is_some() {
                        return (res_a, res_b);
                    }
                }
            }
            read_pos += to_read as u64;
        }
        (res_a, res_b)
    }

    /// Primary rule: DELETE ONLY WHAT THE USER TARGETED.
    /// Handles single character, word, single line, whole multiline, and partial multiline.
    pub fn delete_selection(&mut self) {
        // Ensure all pending line edits are flushed to the piece table first,
        // so byte offsets are consistent with the piece table's virtual document.
        self.flush_all_pending_edits();

        let Some(((start_line, start_col), (end_line, end_col))) = self.char_selection_range() else {
            return;
        };

        if start_line == end_line && start_col == end_col {
            return;
        }

        // Expand selection to include folded (hidden) line ranges.
        // When a block is folded, only the header line is visible. Deleting
        // the header must also delete all the hidden child lines.
        let mut end_line = end_line;
        for (idx, line) in self.viewport.lines.iter().enumerate() {
            if line.line_number >= start_line && line.line_number <= end_line {
                if self.folded_lines.contains(&line.line_number) {
                    if let Some(fold_end) = Self::find_folding_end(&self.viewport.lines, idx) {
                        if fold_end > end_line {
                            end_line = fold_end;
                        }
                    }
                }
            }
        }

        let (Some(ref mut doc), Some(ref engine), Some(ref line_index)) = (&mut self.document, &self.engine, &self.line_index) else {
            return;
        };

        if start_line == end_line && start_col == end_col {
            return;
        }

        let is_dirty = doc.is_dirty();

        // --- Case 1: Single line range deletion ---
        if start_line == end_line {
            let line_no = start_line;
            let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else {
                return;
            };

            if start_col == 0 && end_col == usize::MAX {
                // Entire line deletion
                let (start_offset, end_offset) = if is_dirty {
                    let (so, eo) = Self::piece_table_line_offsets_pair(
                        &doc.piece_table, engine, line_no, line_no + 1,
                    );
                    (
                        so.unwrap_or(self.viewport.lines[idx].byte_offset),
                        eo.unwrap_or_else(|| doc.piece_table.total_length()),
                    )
                } else {
                    let so = self.viewport.lines[idx].byte_offset;
                    let eo = self.viewport.lines.get(idx + 1).map(|l| l.byte_offset)
                        .or_else(|| line_index.line_to_byte_offset(engine, line_no + 1))
                        .unwrap_or_else(|| doc.piece_table.total_length());
                    (so, eo)
                };

                let delete_len = end_offset.saturating_sub(start_offset);
                if delete_len > 0 {
                    let mut deleted_bytes = Vec::new();
                    let _ = doc.piece_table.read_range(engine, start_offset, delete_len as usize, &mut deleted_bytes);
                    let deleted_text = String::from_utf8_lossy(&deleted_bytes).into_owned();
                    doc.delete_range(start_offset, delete_len, deleted_text);
                }

                self.viewport.lines.remove(idx);
                doc.total_lines_delta -= 1;
                self.folded_lines.remove(&line_no);
                for line in &mut self.viewport.lines[idx..] {
                    line.line_number -= 1;
                    line.byte_offset = line.byte_offset.saturating_sub(delete_len);
                }

                doc.modified_lines.remove(&line_no);
                let mut shifted = std::collections::HashMap::new();
                for (k, v) in doc.modified_lines.drain() {
                    if k > line_no {
                        shifted.insert(k - 1, v);
                    } else {
                        shifted.insert(k, v);
                    }
                }
                doc.modified_lines = shifted;
            } else {
                // Single character / word / substring within a line: preserve untouched prefix and suffix!
                let curr_text = self.viewport.lines[idx].text.clone();
                let char_count = curr_text.chars().count();
                let s_char = start_col.min(char_count);
                let e_char = end_col.min(char_count);

                if s_char < e_char {
                    let b_start = curr_text.char_indices().nth(s_char).map(|(b, _)| b).unwrap_or(curr_text.len());
                    let b_end = curr_text.char_indices().nth(e_char).map(|(b, _)| b).unwrap_or(curr_text.len());

                    let line_byte_offset = if is_dirty {
                        let (so, _) = Self::piece_table_line_offsets_pair(
                            &doc.piece_table, engine, line_no, line_no,
                        );
                        so.unwrap_or(self.viewport.lines[idx].byte_offset)
                    } else {
                        self.viewport.lines[idx].byte_offset
                    };

                    let delete_offset = line_byte_offset + b_start as u64;
                    let delete_len = (b_end - b_start) as u64;
                    let deleted_text = curr_text[b_start..b_end].to_string();

                    doc.delete_range(delete_offset, delete_len, deleted_text);

                    let new_text = format!("{}{}", &curr_text[..b_start], &curr_text[b_end..]);
                    self.viewport.lines[idx].text = new_text.clone();

                    for line in &mut self.viewport.lines[idx + 1..] {
                        line.byte_offset = line.byte_offset.saturating_sub(delete_len);
                    }

                    doc.modified_lines.insert(line_no, new_text.clone());
                    if self.active_edit_line == Some(line_no) {
                        self.edit_line_buffer = new_text;
                    }
                }
            }

            self.selection_anchor = None;
            self.selection_anchor_col = None;
            self.selection_head = None;
            self.selection_head_col = None;
            if self.viewport.lines.is_empty() {
                self.viewport.lines.push(ViewportLine {
                    line_number: 1,
                    byte_offset: 0,
                    text: String::new(),
                    is_truncated: false,
                });
            }
            self.current_line = start_line.min(self.viewport.lines.len().max(1));
            self.pending_focus_line = Some((self.current_line, start_col));
            return;
        }

        // --- Case 2: Multi-line range deletion (`start_line < end_line`) ---
        if start_col == 0 && end_col == usize::MAX {
            // Whole lines deletion from start_line to end_line
            let (start_offset, end_offset) = if is_dirty {
                let (so, eo) = Self::piece_table_line_offsets_pair(
                    &doc.piece_table, engine, start_line, end_line + 1,
                );
                (
                    so.or_else(|| self.viewport.lines.iter().find(|l| l.line_number == start_line).map(|l| l.byte_offset)),
                    eo.or_else(|| self.viewport.lines.iter().find(|l| l.line_number == end_line + 1).map(|l| l.byte_offset)).unwrap_or_else(|| doc.piece_table.total_length()),
                )
            } else {
                let so = self.viewport.lines.iter().find(|l| l.line_number == start_line).map(|l| l.byte_offset)
                    .or_else(|| line_index.line_to_byte_offset(engine, start_line));
                let eo = self.viewport.lines.iter().find(|l| l.line_number == end_line + 1).map(|l| l.byte_offset)
                    .or_else(|| line_index.line_to_byte_offset(engine, end_line + 1))
                    .unwrap_or_else(|| doc.piece_table.total_length());
                (so, eo)
            };

            let mut actual_deleted_len = 0;
            if let Some(start_off) = start_offset {
                let delete_len = end_offset.saturating_sub(start_off);
                if delete_len > 0 {
                    let mut deleted_bytes = Vec::new();
                    let _ = doc.piece_table.read_range(engine, start_off, delete_len as usize, &mut deleted_bytes);
                    let deleted_text = String::from_utf8_lossy(&deleted_bytes).into_owned();
                    doc.delete_range(start_off, delete_len, deleted_text);
                    actual_deleted_len = delete_len;
                }
            }

            let count = end_line - start_line + 1;
            doc.total_lines_delta -= count as i64;
            self.folded_lines.retain(|&k| k < start_line || k > end_line);
            let mut shifted_folds = HashSet::new();
            for f in self.folded_lines.drain() {
                if f > end_line {
                    shifted_folds.insert(f - count);
                } else {
                    shifted_folds.insert(f);
                }
            }
            self.folded_lines = shifted_folds;

            self.viewport.lines.retain(|l| l.line_number < start_line || l.line_number > end_line);
            for line in &mut self.viewport.lines {
                if line.line_number > end_line {
                    line.line_number -= count;
                    line.byte_offset = line.byte_offset.saturating_sub(actual_deleted_len);
                }
            }

            doc.modified_lines.retain(|&k, _| k < start_line || k > end_line);
            let mut shifted = std::collections::HashMap::new();
            for (k, v) in doc.modified_lines.drain() {
                if k > end_line {
                    shifted.insert(k - count, v);
                } else {
                    shifted.insert(k, v);
                }
            }
            doc.modified_lines = shifted;
        } else {
            // Partial multi-line deletion: preserve prefix of start_line and suffix of end_line
            let start_idx_opt = self.viewport.lines.iter().position(|l| l.line_number == start_line);
            let end_idx_opt = self.viewport.lines.iter().position(|l| l.line_number == end_line);

            if let (Some(s_idx), Some(e_idx)) = (start_idx_opt, end_idx_opt) {
                let start_text = self.viewport.lines[s_idx].text.clone();
                let end_text = self.viewport.lines[e_idx].text.clone();

                let b_start = start_text.char_indices().nth(start_col).map(|(b, _)| b).unwrap_or(start_text.len());
                let b_end = end_text.char_indices().nth(end_col).map(|(b, _)| b).unwrap_or(end_text.len());

                let prefix = &start_text[..b_start];
                let suffix = &end_text[b_end..];
                let merged_text = format!("{}{}", prefix, suffix);

                let line_start_off = if is_dirty {
                    let (so, _) = Self::piece_table_line_offsets_pair(&doc.piece_table, engine, start_line, start_line);
                    so.unwrap_or(self.viewport.lines[s_idx].byte_offset)
                } else {
                    self.viewport.lines[s_idx].byte_offset
                };
                let line_end_off = if is_dirty {
                    let (so, _) = Self::piece_table_line_offsets_pair(&doc.piece_table, engine, end_line, end_line);
                    so.unwrap_or(self.viewport.lines[e_idx].byte_offset)
                } else {
                    self.viewport.lines[e_idx].byte_offset
                };

                let delete_start_off = line_start_off + b_start as u64;
                let delete_end_off = line_end_off + b_end as u64;
                let delete_len = delete_end_off.saturating_sub(delete_start_off);

                if delete_len > 0 {
                    let mut deleted_bytes = Vec::new();
                    let _ = doc.piece_table.read_range(engine, delete_start_off, delete_len as usize, &mut deleted_bytes);
                    let deleted_text = String::from_utf8_lossy(&deleted_bytes).into_owned();
                    doc.delete_range(delete_start_off, delete_len, deleted_text);
                }

                self.viewport.lines[s_idx].text = merged_text.clone();

                let lines_removed = e_idx - s_idx;
                if lines_removed > 0 {
                    doc.total_lines_delta -= lines_removed as i64;
                }
                self.viewport.lines.drain(s_idx + 1..=e_idx);

                for line in &mut self.viewport.lines[s_idx + 1..] {
                    line.line_number -= lines_removed;
                    line.byte_offset = line.byte_offset.saturating_sub(delete_len);
                }

                let mut shifted = std::collections::HashMap::new();
                for (k, v) in doc.modified_lines.drain() {
                    if k > end_line {
                        shifted.insert(k - lines_removed, v);
                    } else if k < start_line {
                        shifted.insert(k, v);
                    }
                }
                shifted.insert(start_line, merged_text.clone());
                doc.modified_lines = shifted;

                if self.active_edit_line == Some(start_line) {
                    self.edit_line_buffer = merged_text;
                }
            }
        }

        self.selection_anchor = None;
        self.selection_anchor_col = None;
        self.selection_head = None;
        self.selection_head_col = None;
        self.active_edit_line = None;
        self.edit_line_buffer.clear();
        self.line_edit_initial_texts.clear();
        if self.viewport.lines.is_empty() {
            self.viewport.lines.push(ViewportLine {
                line_number: 1,
                byte_offset: 0,
                text: String::new(),
                is_truncated: false,
            });
        }
        self.current_line = start_line.min(self.viewport.lines.len().max(1));
        self.pending_focus_line = Some((self.current_line, start_col));
    }

    /// Delete selected column ranges on multiple lines while preserving untouched line breaks (Section 24).
    pub fn delete_partial_lines_selection(&mut self, line_ranges: &[(usize, usize, usize)]) {
        let (Some(ref mut doc), _engine, _line_index) = (&mut self.document, &self.engine, &self.line_index) else {
            return;
        };

        let mut sorted_ranges = line_ranges.to_vec();
        sorted_ranges.sort_by_key(|&(l, _, _)| std::cmp::Reverse(l));

        for (line_no, col_start, col_end) in sorted_ranges {
            if let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) {
                let curr_text = self.viewport.lines[idx].text.clone();
                let char_count = curr_text.chars().count();
                let s_char = col_start.min(char_count);
                let e_char = col_end.min(char_count);
                if s_char < e_char {
                    let b_start = curr_text.char_indices().nth(s_char).map(|(b, _)| b).unwrap_or(curr_text.len());
                    let b_end = curr_text.char_indices().nth(e_char).map(|(b, _)| b).unwrap_or(curr_text.len());

                    let delete_offset = self.viewport.lines[idx].byte_offset + b_start as u64;
                    let delete_len = (b_end - b_start) as u64;
                    let deleted_text = curr_text[b_start..b_end].to_string();

                    doc.delete_range(delete_offset, delete_len, deleted_text);

                    let new_text = format!("{}{}", &curr_text[..b_start], &curr_text[b_end..]);
                    self.viewport.lines[idx].text = new_text.clone();

                    for line in &mut self.viewport.lines[idx + 1..] {
                        line.byte_offset = line.byte_offset.saturating_sub(delete_len);
                    }

                    doc.modified_lines.insert(line_no, new_text);
                }
            }
        }

        self.selection_anchor = None;
        self.selection_anchor_col = None;
        self.selection_head = None;
        self.selection_head_col = None;
    }

    pub fn cursor_pos(&self) -> (usize, usize) {
        let line = self.current_line;
        let col = self.pending_focus_line.and_then(|(l, c)| if l == line { Some(c) } else { None })
            .or_else(|| self.selection_head_col)
            .unwrap_or(0);
        (line, col)
    }

    pub fn set_cursor(&mut self, line: usize, col: usize) {
        let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0).max(self.viewport.lines.len()).max(1);
        let target_line = line.max(1).min(total);
        self.current_line = target_line;
        let max_col = self.viewport.lines.iter().find(|l| l.line_number == target_line)
            .map(|l| l.text.chars().count())
            .unwrap_or(0);
        let target_col = col.min(max_col);
        self.pending_focus_line = Some((target_line, target_col));
        self.active_edit_line = Some(target_line);
        if let Some(l) = self.viewport.lines.iter().find(|l| l.line_number == target_line) {
            self.edit_line_buffer = l.text.clone();
        }
        self.selection_anchor = None;
        self.selection_anchor_col = None;
        self.selection_head = None;
        self.selection_head_col = None;
    }

    pub fn select_range(&mut self, anchor_line: usize, anchor_col: usize, head_line: usize, head_col: usize) {
        self.selection_anchor = Some(anchor_line);
        self.selection_anchor_col = Some(anchor_col);
        self.selection_head = Some(head_line);
        self.selection_head_col = Some(head_col);
        self.current_line = head_line;
        self.pending_focus_line = Some((head_line, head_col));
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_anchor_col = None;
        self.selection_head = None;
        self.selection_head_col = None;
    }

    pub fn move_cursor_left(&mut self) {
        let (line, col) = self.cursor_pos();
        if col > 0 {
            self.set_cursor(line, col - 1);
        } else if line > 1 {
            let prev_line_len = self.viewport.lines.iter().find(|l| l.line_number == line - 1)
                .map(|l| l.text.chars().count())
                .unwrap_or(0);
            self.set_cursor(line - 1, prev_line_len);
        }
    }

    pub fn move_cursor_right(&mut self) {
        let (line, col) = self.cursor_pos();
        let curr_line_len = self.viewport.lines.iter().find(|l| l.line_number == line)
            .map(|l| l.text.chars().count())
            .unwrap_or(0);
        let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0).max(self.viewport.lines.len()).max(1);
        if col < curr_line_len {
            self.set_cursor(line, col + 1);
        } else if line < total {
            self.set_cursor(line + 1, 0);
        }
    }

    pub fn move_cursor_up(&mut self) {
        let (line, col) = self.cursor_pos();
        if line > 1 {
            self.set_cursor(line - 1, col);
        }
    }

    pub fn move_cursor_down(&mut self) {
        let (line, col) = self.cursor_pos();
        let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0).max(self.viewport.lines.len()).max(1);
        if line < total {
            self.set_cursor(line + 1, col);
        }
    }

    pub fn move_cursor_home(&mut self) {
        let (line, _) = self.cursor_pos();
        self.set_cursor(line, 0);
    }

    pub fn move_cursor_end(&mut self) {
        let (line, _) = self.cursor_pos();
        let curr_line_len = self.viewport.lines.iter().find(|l| l.line_number == line)
            .map(|l| l.text.chars().count())
            .unwrap_or(0);
        self.set_cursor(line, curr_line_len);
    }

    pub fn move_cursor_doc_start(&mut self) {
        self.set_cursor(1, 0);
    }

    pub fn move_cursor_doc_end(&mut self) {
        let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0).max(self.viewport.lines.len()).max(1);
        let last_line_len = self.viewport.lines.iter().find(|l| l.line_number == total)
            .map(|l| l.text.chars().count())
            .unwrap_or(0);
        self.set_cursor(total, last_line_len);
    }

    pub fn type_text_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        if self.selection_anchor.is_some() || self.selection_head.is_some() {
            self.delete_selection();
        }

        if self.viewport.lines.is_empty() {
            self.viewport.lines.push(ViewportLine {
                line_number: 1,
                byte_offset: 0,
                text: String::new(),
                is_truncated: false,
            });
            self.current_line = 1;
        }

        let (line_no, cursor_col) = self.cursor_pos();

        if text.contains('\n') {
            let normalized = text.replace("\r\n", "\n");
            let chunks: Vec<&str> = normalized.split('\n').collect();

            let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else {
                return;
            };

            let full_text = self.viewport.lines[idx].text.clone();
            let chars: Vec<char> = full_text.chars().collect();
            let split_idx = cursor_col.min(chars.len());
            let before: String = chars[..split_idx].iter().collect();
            let after: String = chars[split_idx..].iter().collect();

            let line_byte_off = self.viewport.lines[idx].byte_offset;
            let insert_off = line_byte_off + before.as_bytes().len() as u64;

            if let Some(ref mut doc) = self.document {
                doc.insert_text(insert_off, text.to_string());
            }

            let first_line_text = format!("{}{}", before, chunks[0]);
            self.viewport.lines[idx].text = first_line_text.clone();

            let mut cur_off = insert_off + chunks[0].as_bytes().len() as u64 + 1;
            let mut inserted_lines = Vec::new();

            for (i, &chunk) in chunks[1..chunks.len() - 1].iter().enumerate() {
                inserted_lines.push(ViewportLine {
                    line_number: line_no + 1 + i,
                    byte_offset: cur_off,
                    text: chunk.to_string(),
                    is_truncated: false,
                });
                cur_off += chunk.as_bytes().len() as u64 + 1;
            }

            let last_chunk = chunks.last().unwrap();
            let last_line_text = format!("{}{}", last_chunk, after);
            let last_line_no = line_no + chunks.len() - 1;
            inserted_lines.push(ViewportLine {
                line_number: last_line_no,
                byte_offset: cur_off,
                text: last_line_text.clone(),
                is_truncated: false,
            });

            let added_count = chunks.len() - 1;
            let text_byte_len = text.as_bytes().len() as u64;

            for l in &mut self.viewport.lines[idx + 1..] {
                l.line_number += added_count;
                l.byte_offset += text_byte_len;
            }

            for (offset, line) in inserted_lines.into_iter().enumerate() {
                self.viewport.lines.insert(idx + 1 + offset, line);
            }

            if let Some(ref mut doc) = self.document {
                doc.modified_lines.insert(line_no, first_line_text);
                for (i, &chunk) in chunks[1..chunks.len() - 1].iter().enumerate() {
                    doc.modified_lines.insert(line_no + 1 + i, chunk.to_string());
                }
                doc.modified_lines.insert(last_line_no, last_line_text);
            }

            let target_col = last_chunk.chars().count();
            self.set_cursor(last_line_no, target_col);
        } else {
            let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else {
                return;
            };

            let curr_text = self.viewport.lines[idx].text.clone();
            let char_count = curr_text.chars().count();
            let insert_col = cursor_col.min(char_count);
            let b_off = curr_text.char_indices().nth(insert_col).map(|(b, _)| b).unwrap_or(curr_text.len());

            let insert_offset = self.viewport.lines[idx].byte_offset + b_off as u64;
            let insert_len = text.as_bytes().len() as u64;

            if let Some(ref mut doc) = self.document {
                doc.insert_text(insert_offset, text.to_string());
            }

            let new_text = format!("{}{}{}", &curr_text[..b_off], text, &curr_text[b_off..]);
            self.viewport.lines[idx].text = new_text.clone();

            for line in &mut self.viewport.lines[idx + 1..] {
                line.byte_offset += insert_len;
            }

            if let Some(ref mut doc) = self.document {
                doc.modified_lines.insert(line_no, new_text.clone());
            }

            self.edit_line_buffer = new_text;
            self.set_cursor(line_no, insert_col + text.chars().count());
        }
    }

    pub fn paste_text_at_cursor(&mut self, text: &str) {
        self.type_text_at_cursor(text);
    }

    pub fn backspace_at_cursor(&mut self) {
        if self.selection_anchor.is_some() || self.selection_head.is_some() {
            self.delete_selection();
            return;
        }

        if self.viewport.lines.is_empty() {
            return;
        }

        let (line_no, col) = self.cursor_pos();
        if col == 0 {
            if line_no > 1 {
                self.merge_line_with_previous(line_no);
            }
            return;
        }

        let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else {
            return;
        };

        let curr_text = self.viewport.lines[idx].text.clone();
        let char_count = curr_text.chars().count();
        if col > char_count {
            return;
        }

        let b_start = curr_text.char_indices().nth(col - 1).map(|(b, _)| b).unwrap_or(0);
        let b_end = curr_text.char_indices().nth(col).map(|(b, _)| b).unwrap_or(curr_text.len());

        let del_len = (b_end - b_start) as u64;
        let del_off = self.viewport.lines[idx].byte_offset + b_start as u64;
        let deleted_str = curr_text[b_start..b_end].to_string();

        if let Some(ref mut doc) = self.document {
            doc.delete_range(del_off, del_len, deleted_str);
        }

        let new_text = format!("{}{}", &curr_text[..b_start], &curr_text[b_end..]);
        self.viewport.lines[idx].text = new_text.clone();

        for line in &mut self.viewport.lines[idx + 1..] {
            line.byte_offset = line.byte_offset.saturating_sub(del_len);
        }

        if let Some(ref mut doc) = self.document {
            doc.modified_lines.insert(line_no, new_text.clone());
        }

        self.edit_line_buffer = new_text;
        self.set_cursor(line_no, col - 1);
    }

    pub fn delete_at_cursor(&mut self) {
        if self.selection_anchor.is_some() || self.selection_head.is_some() {
            self.delete_selection();
            return;
        }

        if self.viewport.lines.is_empty() {
            return;
        }

        let (line_no, col) = self.cursor_pos();
        let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else {
            return;
        };

        let curr_text = self.viewport.lines[idx].text.clone();
        let char_count = curr_text.chars().count();

        if col >= char_count {
            let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0).max(self.viewport.lines.len()).max(1);
            if line_no < total {
                self.merge_line_with_previous(line_no + 1);
            }
            return;
        }

        let b_start = curr_text.char_indices().nth(col).map(|(b, _)| b).unwrap_or(curr_text.len());
        let b_end = curr_text.char_indices().nth(col + 1).map(|(b, _)| b).unwrap_or(curr_text.len());

        let del_len = (b_end - b_start) as u64;
        let del_off = self.viewport.lines[idx].byte_offset + b_start as u64;
        let deleted_str = curr_text[b_start..b_end].to_string();

        if let Some(ref mut doc) = self.document {
            doc.delete_range(del_off, del_len, deleted_str);
        }

        let new_text = format!("{}{}", &curr_text[..b_start], &curr_text[b_end..]);
        self.viewport.lines[idx].text = new_text.clone();

        for line in &mut self.viewport.lines[idx + 1..] {
            line.byte_offset = line.byte_offset.saturating_sub(del_len);
        }

        if let Some(ref mut doc) = self.document {
            doc.modified_lines.insert(line_no, new_text.clone());
        }

        self.edit_line_buffer = new_text;
        self.set_cursor(line_no, col);
    }

    pub fn split_line_at_cursor(&mut self, line_no: usize, cursor_col: usize) {
        let Some(ref mut doc) = self.document else { return; };
        let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else { return; };
        let line_byte_off = self.viewport.lines[idx].byte_offset;
        let full_text = self.viewport.lines[idx].text.clone();

        let chars: Vec<char> = full_text.chars().collect();
        let split_idx = cursor_col.min(chars.len());
        let before: String = chars[..split_idx].iter().collect();
        let after: String = chars[split_idx..].iter().collect();

        let before_bytes_len = before.as_bytes().len() as u64;
        let split_offset = line_byte_off + before_bytes_len;

        // Insert newline at split_offset
        doc.insert_text(split_offset, "\n".to_string());

        // Update viewport lines
        self.viewport.lines[idx].text = before.clone();
        let new_line_off = split_offset + 1;
        let new_line = ViewportLine {
            line_number: line_no + 1,
            byte_offset: new_line_off,
            text: after.clone(),
            is_truncated: false,
        };
        self.viewport.lines.insert(idx + 1, new_line);

        // Shift subsequent lines (line_number by +1, byte_offset by +1 for newline)
        for l in &mut self.viewport.lines[idx + 2..] {
            l.line_number += 1;
            l.byte_offset += 1;
        }

        // Shift doc.modified_lines
        let mut shifted = std::collections::HashMap::new();
        for (k, v) in doc.modified_lines.drain() {
            if k > line_no {
                shifted.insert(k + 1, v);
            } else if k < line_no {
                shifted.insert(k, v);
            }
        }
        shifted.insert(line_no, before);
        shifted.insert(line_no + 1, after.clone());
        doc.modified_lines = shifted;

        self.active_edit_line = Some(line_no + 1);
        self.edit_line_buffer = after.clone();
        self.line_edit_initial_texts.insert(line_no + 1, after);
        self.current_line = line_no + 1;
        self.pending_focus_line = Some((line_no + 1, 0));
    }

    pub fn merge_line_with_previous(&mut self, line_no: usize) {
        if line_no <= 1 {
            return;
        }
        let Some(ref mut doc) = self.document else { return; };
        let Some(idx) = self.viewport.lines.iter().position(|l| l.line_number == line_no) else { return; };
        if idx == 0 || self.viewport.lines[idx - 1].line_number != line_no - 1 {
            return;
        }

        let prev_text = self.viewport.lines[idx - 1].text.clone();
        let curr_text = self.viewport.lines[idx].text.clone();
        let prev_byte_off = self.viewport.lines[idx - 1].byte_offset;
        let curr_byte_off = self.viewport.lines[idx].byte_offset;

        let newline_len = curr_byte_off.saturating_sub(prev_byte_off + prev_text.as_bytes().len() as u64).max(1);
        let newline_off = prev_byte_off + prev_text.as_bytes().len() as u64;

        let merged = format!("{}{}", prev_text, curr_text);
        let prev_len_chars = prev_text.chars().count();

        // Delete newline between them
        let mut deleted_bytes = Vec::new();
        if let Some(ref engine) = self.engine {
            let _ = doc.piece_table.read_range(engine, newline_off, newline_len as usize, &mut deleted_bytes);
        }
        let del_str = if deleted_bytes.is_empty() { "\n".to_string() } else { String::from_utf8_lossy(&deleted_bytes).into_owned() };
        doc.delete_range(newline_off, newline_len, del_str);

        // Update viewport lines
        self.viewport.lines[idx - 1].text = merged.clone();
        self.viewport.lines.remove(idx);

        // Shift subsequent lines
        for l in &mut self.viewport.lines[idx..] {
            l.line_number -= 1;
            l.byte_offset = l.byte_offset.saturating_sub(newline_len);
        }

        // Shift doc.modified_lines
        let mut shifted = std::collections::HashMap::new();
        for (k, v) in doc.modified_lines.drain() {
            if k > line_no {
                shifted.insert(k - 1, v);
            } else if k < line_no - 1 {
                shifted.insert(k, v);
            }
        }
        shifted.insert(line_no - 1, merged.clone());
        doc.modified_lines = shifted;

        self.active_edit_line = Some(line_no - 1);
        self.edit_line_buffer = merged.clone();
        self.line_edit_initial_texts.insert(line_no - 1, merged);
        self.current_line = line_no - 1;
        self.pending_focus_line = Some((line_no - 1, prev_len_chars));
    }

    pub fn move_selection_up(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            if start > 1 {
                if let Some(doc) = self.document.as_mut() {
                    let prev_line = self.viewport.lines.iter().find(|l| l.line_number == start - 1).map(|l| (l.line_number, l.byte_offset, l.text.clone()));
                    let curr_first = self.viewport.lines.iter().find(|l| l.line_number == start).map(|l| (l.line_number, l.byte_offset, l.text.clone()));
                    if let (Some((p_num, p_off, p_txt)), Some((c_num, c_off, c_txt))) = (prev_line, curr_first) {
                        let op1 = crate::editor::document::EditOperation::ReplaceLine {
                            line_number: p_num,
                            byte_offset: p_off,
                            old_text: p_txt.clone(),
                            new_text: c_txt.clone(),
                        };
                        let op2 = crate::editor::document::EditOperation::ReplaceLine {
                            line_number: c_num,
                            byte_offset: c_off,
                            old_text: c_txt,
                            new_text: p_txt,
                        };
                        doc.apply_batch(vec![op1, op2]);
                    }
                }
                self.selection_anchor = Some(start - 1);
                self.selection_head = Some(end - 1);
                if let (Some(eng), Some(idx)) = (&self.engine, &self.line_index) {
                    self.viewport.load_lines_indexed(eng, idx, self.current_line, VISIBLE_LINE_BUFFER);
                    if let Some(ref doc) = self.document {
                        self.viewport.apply_line_overrides(&doc.modified_lines);
                    }
                }
            }
        }
    }

    pub fn move_selection_down(&mut self) {
        let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(1);
        if let Some((start, end)) = self.selection_range() {
            if end < total {
                if let Some(doc) = self.document.as_mut() {
                    let next_line = self.viewport.lines.iter().find(|l| l.line_number == end + 1).map(|l| (l.line_number, l.byte_offset, l.text.clone()));
                    let curr_last = self.viewport.lines.iter().find(|l| l.line_number == end).map(|l| (l.line_number, l.byte_offset, l.text.clone()));
                    if let (Some((n_num, n_off, n_txt)), Some((c_num, c_off, c_txt))) = (next_line, curr_last) {
                        let op1 = crate::editor::document::EditOperation::ReplaceLine {
                            line_number: n_num,
                            byte_offset: n_off,
                            old_text: n_txt.clone(),
                            new_text: c_txt.clone(),
                        };
                        let op2 = crate::editor::document::EditOperation::ReplaceLine {
                            line_number: c_num,
                            byte_offset: c_off,
                            old_text: c_txt,
                            new_text: n_txt,
                        };
                        doc.apply_batch(vec![op1, op2]);
                    }
                }
                self.selection_anchor = Some(start + 1);
                self.selection_head = Some(end + 1);
                if let (Some(eng), Some(idx)) = (&self.engine, &self.line_index) {
                    self.viewport.load_lines_indexed(eng, idx, self.current_line, VISIBLE_LINE_BUFFER);
                    if let Some(ref doc) = self.document {
                        self.viewport.apply_line_overrides(&doc.modified_lines);
                    }
                }
            }
        }
    }

    pub fn indent_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            if self.document.is_none() {
                if let Some(ref eng) = self.engine {
                    self.document = Some(EditorDocument::new(eng.size()));
                }
            }
            let mut ops = Vec::new();
            for line_no in start..=end {
                if let Some(line) = self.viewport.lines.iter().find(|l| l.line_number == line_no) {
                    let old_text = line.text.clone();
                    let new_text = format!("  {}", old_text);
                    ops.push(crate::editor::document::EditOperation::ReplaceLine {
                        line_number: line.line_number,
                        byte_offset: line.byte_offset,
                        old_text,
                        new_text,
                    });
                }
            }
            if !ops.is_empty() {
                if let Some(ref mut doc) = self.document {
                    doc.apply_batch(ops);
                    self.viewport.apply_line_overrides(&doc.modified_lines);
                }
                for line in &mut self.viewport.lines {
                    if line.line_number >= start && line.line_number <= end {
                        if let Some(ref doc) = self.document {
                            if let Some(mod_text) = doc.modified_lines.get(&line.line_number) {
                                line.text = mod_text.clone();
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn unindent_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            if self.document.is_none() {
                if let Some(ref eng) = self.engine {
                    self.document = Some(EditorDocument::new(eng.size()));
                }
            }
            let mut ops = Vec::new();
            for line_no in start..=end {
                if let Some(line) = self.viewport.lines.iter().find(|l| l.line_number == line_no) {
                    let old_text = line.text.clone();
                    let new_text = if old_text.starts_with("  ") {
                        old_text[2..].to_string()
                    } else if old_text.starts_with(' ') {
                        old_text[1..].to_string()
                    } else if old_text.starts_with('\t') {
                        old_text[1..].to_string()
                    } else {
                        old_text.clone()
                    };
                    if old_text != new_text {
                        ops.push(crate::editor::document::EditOperation::ReplaceLine {
                            line_number: line.line_number,
                            byte_offset: line.byte_offset,
                            old_text,
                            new_text,
                        });
                    }
                }
            }
            if !ops.is_empty() {
                if let Some(ref mut doc) = self.document {
                    doc.apply_batch(ops);
                    self.viewport.apply_line_overrides(&doc.modified_lines);
                }
                for line in &mut self.viewport.lines {
                    if line.line_number >= start && line.line_number <= end {
                        if let Some(ref doc) = self.document {
                            if let Some(mod_text) = doc.modified_lines.get(&line.line_number) {
                                line.text = mod_text.clone();
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn transform_selection_case(&mut self, ctx: &egui::Context, to_upper: bool) {
        if self.document.is_none() {
            let size = self.engine.as_ref().map(|e| e.size()).unwrap_or(0);
            self.document = Some(crate::editor::document::EditorDocument::new(size));
        }

        let char_to_byte_offset = |s: &str, char_idx: usize| -> usize {
            if char_idx == 0 {
                0
            } else {
                s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
            }
        };

        // Case 1: Check if currently editing a line and has selected characters in TextEdit
        if let Some(active_line) = self.active_edit_line {
            let edit_id = egui::Id::new("line_editor").with(active_line);
            let state_opt = egui::text_edit::TextEditState::load(ctx, edit_id);
            let char_range = state_opt.as_ref().and_then(|state| state.cursor.char_range());
            if let Some(r) = char_range {
                let s_char = r.primary.index.min(r.secondary.index);
                let e_char = r.primary.index.max(r.secondary.index);
                if s_char != e_char {
                    let b_start = char_to_byte_offset(&self.edit_line_buffer, s_char);
                    let b_end = char_to_byte_offset(&self.edit_line_buffer, e_char);
                    if b_start < b_end && b_end <= self.edit_line_buffer.len() {
                        let orig_slice = &self.edit_line_buffer[b_start..b_end];
                        let transformed = if to_upper { orig_slice.to_uppercase() } else { orig_slice.to_lowercase() };
                        let old_text = self.edit_line_buffer.clone();
                        self.edit_line_buffer.replace_range(b_start..b_end, &transformed);
                        let new_text = self.edit_line_buffer.clone();

                        self.line_edit_initial_texts.entry(active_line).or_insert(old_text);
                        if let Some(ref mut doc) = self.document {
                            doc.modified_lines.insert(active_line, new_text.clone());
                        }
                        for line in &mut self.viewport.lines {
                            if line.line_number == active_line {
                                line.text = new_text.clone();
                                break;
                            }
                        }

                        if let Some(mut state) = state_opt {
                            let new_e_char = s_char + transformed.chars().count();
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(s_char),
                                egui::text::CCursor::new(new_e_char),
                            )));
                            state.store(ctx, edit_id);
                        }

                        if self.auto_save {
                            self.auto_save_timer = Some(Instant::now());
                        }
                        ctx.request_repaint();
                        return;
                    }
                }
            }

            // If no range selected in active line's TextEdit, check if 2D selection exists
            let has_range_sel = self.char_selection_range().map_or(false, |((sl, sc), (el, ec))| !(sl == el && sc == ec));
            if !has_range_sel {
                let old_text = self.edit_line_buffer.clone();
                let new_text = if to_upper { old_text.to_uppercase() } else { old_text.to_lowercase() };
                if old_text != new_text {
                    self.edit_line_buffer = new_text.clone();
                    self.line_edit_initial_texts.entry(active_line).or_insert(old_text);
                    if let Some(ref mut doc) = self.document {
                        doc.modified_lines.insert(active_line, new_text.clone());
                    }
                    for line in &mut self.viewport.lines {
                        if line.line_number == active_line {
                            line.text = new_text.clone();
                            break;
                        }
                    }
                    if self.auto_save {
                        self.auto_save_timer = Some(Instant::now());
                    }
                    ctx.request_repaint();
                }
                return;
            }
        }

        // Case 2: Multi-line or mouse-drag character selection active
        if let Some(((start_line, start_col), (end_line, end_col))) = self.char_selection_range() {
            if !(start_line == end_line && start_col == end_col) {
                let mut ops = Vec::new();
                for line in &mut self.viewport.lines {
                    if line.line_number >= start_line && line.line_number <= end_line {
                        let sel = if start_line == end_line {
                            let b_start = char_to_byte_offset(&line.text, start_col);
                            let b_end = char_to_byte_offset(&line.text, end_col);
                            if b_start < b_end { Some((b_start, b_end)) } else { None }
                        } else if line.line_number == start_line {
                            let b_start = char_to_byte_offset(&line.text, start_col);
                            if b_start < line.text.len() { Some((b_start, line.text.len())) } else { None }
                        } else if line.line_number == end_line {
                            let b_end = char_to_byte_offset(&line.text, end_col);
                            if b_end > 0 { Some((0, b_end)) } else { None }
                        } else {
                            Some((0, line.text.len()))
                        };

                        if let Some((b_start, b_end)) = sel {
                            if b_start < b_end && b_end <= line.text.len() {
                                let orig_slice = &line.text[b_start..b_end];
                                let transformed = if to_upper { orig_slice.to_uppercase() } else { orig_slice.to_lowercase() };
                                if transformed != orig_slice {
                                    let old_line = line.text.clone();
                                    let mut new_line = line.text.clone();
                                    new_line.replace_range(b_start..b_end, &transformed);
                                    ops.push(crate::editor::document::EditOperation::ReplaceLine {
                                        line_number: line.line_number,
                                        byte_offset: line.byte_offset,
                                        old_text: old_line,
                                        new_text: new_line.clone(),
                                    });
                                    line.text = new_line;
                                }
                            }
                        }
                    }
                }
                if !ops.is_empty() {
                    if let Some(ref mut doc) = self.document {
                        doc.apply_batch(ops);
                    }
                    if self.auto_save {
                        self.auto_save_timer = Some(Instant::now());
                    }
                    ctx.request_repaint();
                }
                return;
            }
        }

        // Case 3: No selection, transform current line
        let curr_line = self.current_line;
        if let Some(line) = self.viewport.lines.iter_mut().find(|l| l.line_number == curr_line) {
            let transformed = if to_upper { line.text.to_uppercase() } else { line.text.to_lowercase() };
            if transformed != line.text {
                let (num, off, old) = (line.line_number, line.byte_offset, line.text.clone());
                line.text = transformed.clone();
                if let Some(ref mut doc) = self.document {
                    doc.edit_line(num, off, &old, &transformed);
                }
                if self.auto_save {
                    self.auto_save_timer = Some(Instant::now());
                }
                ctx.request_repaint();
            }
        }
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
                self.is_edit_mode = true;
                self.active_edit_line = None;
                self.edit_line_buffer.clear();
                self.folded_lines.clear();
                self.selection_anchor = None;
                self.selection_head = None;

                self.engine = Some(Arc::clone(&engine));
                self.line_index = Some(Arc::clone(&line_index));
                self.indexer_cancel = Some(Arc::clone(&cancel));
                self.indexer_handle = Some(handle);
                self.file_type = Some(file_type);
                self.open_duration = Some(dur);
                let sample_len = (engine.size().min(524288)) as usize;
                let sample = engine.read_range(0, sample_len).unwrap_or(&[]);
                self.discovered_tags = crate::formats::DiscoveredTags::from_sample(sample, Some(file_type));
                self.show_query_suggestions = false;
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
                    is_edit_mode: true,
                    active_edit_line: None,
                    edit_line_buffer: String::new(),
                    xml_tree_root: None,
                    json_tree_root: None,
                    folded_lines: HashSet::new(),
                    selection_anchor: None,
                    selection_head: None,
                    current_xpath: None,
                };
                self.tabs.push(new_tab);

                // Auto-detect CSV/TSV
                let ext = path_buf.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "csv" {
                    self.csv_grid.is_enabled = true;
                    self.csv_grid.delimiter = ',';
                    self.csv_grid.headers = self.viewport.lines.first()
                        .map(|l| split_delimited(&l.text, ','))
                        .unwrap_or_default();
                } else if ext == "tsv" {
                    self.csv_grid.is_enabled = true;
                    self.csv_grid.delimiter = '\t';
                    self.csv_grid.headers = self.viewport.lines.first()
                        .map(|l| split_delimited(&l.text, '\t'))
                        .unwrap_or_default();
                } else {
                    self.csv_grid.is_enabled = false;
                    self.csv_grid.headers.clear();
                }

                // Auto-build compact structure tree if size <= 2GB
                if file_type == FileType::Xml {
                    self.fix_malformed_xml_declaration();
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
                tab.folded_lines = self.folded_lines.clone();
                tab.selection_anchor = self.selection_anchor;
                tab.selection_head = self.selection_head;
                tab.current_xpath = self.current_xpath.clone();
            }
        }
    }

    pub fn switch_to_tab(&mut self, tab_id: usize) {
        if self.active_tab_id == Some(tab_id) && self.engine.is_some() {
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
            self.folded_lines = tab.folded_lines.clone();
            self.selection_anchor = tab.selection_anchor;
            self.selection_head = tab.selection_head;
            self.current_xpath = tab.current_xpath.clone();
            self.last_resolved_line = 0;

            self.search_matches.write().unwrap().clear();
            *self.search_status.write().unwrap() = SearchStatus::Idle;
            self.active_match_idx = None;

            let ext = tab.path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if ext == "csv" {
                self.csv_grid.is_enabled = true;
                self.csv_grid.delimiter = ',';
                self.csv_grid.headers.clear();
            } else if ext == "tsv" {
                self.csv_grid.is_enabled = true;
                self.csv_grid.delimiter = '\t';
                self.csv_grid.headers.clear();
            } else {
                self.csv_grid.is_enabled = false;
                self.csv_grid.headers.clear();
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
            self.scroll_to_line_with_headroom(line, 4);
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

    pub fn get_word_or_selection_at_cursor(&self, ctx: &egui::Context) -> Option<String> {
        let line_no = self.active_edit_line.unwrap_or(self.current_line);
        let edit_id = egui::Id::new("line_editor").with(line_no);
        let line_text = self.get_line_text_for_resolver(line_no)?;

        if let Some(state) = egui::text_edit::TextEditState::load(ctx, edit_id) {
            if let Some(r) = state.cursor.char_range() {
                let s = r.primary.index.min(r.secondary.index);
                let e = r.primary.index.max(r.secondary.index);
                let chars: Vec<char> = line_text.chars().collect();
                if s < e && e <= chars.len() {
                    let selected: String = chars[s..e].iter().collect();
                    if !selected.trim().is_empty() {
                        return Some(selected);
                    }
                } else if s <= chars.len() {
                    let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '$' || c == '@' || c == ':';
                    let mut start = s;
                    if (start == chars.len() || !is_word_char(chars[start])) && start > 0 && is_word_char(chars[start - 1]) {
                        start -= 1;
                    }
                    if start < chars.len() && is_word_char(chars[start]) {
                        let mut end = start;
                        while start > 0 && is_word_char(chars[start - 1]) {
                            start -= 1;
                        }
                        while end < chars.len() && is_word_char(chars[end]) {
                            end += 1;
                        }
                        let word: String = chars[start..end].iter().collect();
                        if !word.trim().is_empty() {
                            return Some(word);
                        }
                    }
                }
            }
        }

        if let Some(col) = self.last_resolved_col {
            let chars: Vec<char> = line_text.chars().collect();
            let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '$' || c == '@' || c == ':';
            let mut start = col.min(chars.len());
            if (start == chars.len() || (start < chars.len() && !is_word_char(chars[start]))) && start > 0 && is_word_char(chars[start - 1]) {
                start -= 1;
            }
            if start < chars.len() && is_word_char(chars[start]) {
                let mut end = start;
                while start > 0 && is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                while end < chars.len() && is_word_char(chars[end]) {
                    end += 1;
                }
                let word: String = chars[start..end].iter().collect();
                if !word.trim().is_empty() {
                    return Some(word);
                }
            }
        }

        if !self.search_query.pattern.trim().is_empty() {
            return Some(self.search_query.pattern.clone());
        }

        None
    }

    pub fn select_next_occurrence(&mut self, ctx: &egui::Context) {
        if self.engine.is_none() {
            return;
        }
        if let Some(word) = self.get_word_or_selection_at_cursor(ctx) {
            if self.search_query.pattern != word || self.search_query.is_empty() {
                self.search_query.pattern = word;
                self.show_search_bar = true;
                self.start_search();
                return;
            }
        }
        self.find_next();
    }

    pub fn select_all_occurrences(&mut self, ctx: &egui::Context) {
        if self.engine.is_none() {
            return;
        }
        if let Some(word) = self.get_word_or_selection_at_cursor(ctx) {
            self.search_query.pattern = word.clone();
            self.show_search_bar = true;
            self.show_replace_bar = true;
            self.focus_replace_input = true;
            self.start_search();
            self.status_notification = Some((
                format!("Selected all occurrences of '{}' — ready to replace", word),
                Instant::now(),
            ));
        } else if !self.search_query.is_empty() {
            self.show_search_bar = true;
            self.show_replace_bar = true;
            self.focus_replace_input = true;
        }
    }

    pub fn toggle_find_and_replace(&mut self, ctx: &egui::Context) {
        if self.engine.is_none() {
            return;
        }
        if !self.show_search_bar || !self.show_replace_bar {
            if let Some(word) = self.get_word_or_selection_at_cursor(ctx) {
                if self.search_query.pattern.is_empty() {
                    self.search_query.pattern = word;
                }
            }
            self.show_search_bar = true;
            self.show_replace_bar = true;
            if self.search_query.is_empty() {
                self.focus_search_input = true;
            } else {
                self.focus_replace_input = true;
            }
        } else {
            self.show_replace_bar = false;
        }
    }

    pub fn replace_current_match(&mut self) {
        if self.search_query.is_empty() || self.engine.is_none() {
            return;
        }
        if self.document.is_none() {
            if let Some(ref eng) = self.engine {
                self.document = Some(EditorDocument::new(eng.size()));
            }
        }

        let current_idx = match self.active_match_idx {
            Some(idx) => idx,
            None => {
                self.find_next();
                if let Some(idx) = self.active_match_idx {
                    idx
                } else {
                    return;
                }
            }
        };

        let match_item = self.search_matches.read().unwrap().get(current_idx).cloned();
        if let Some(m) = match_item {
            let line_no = m.line_number;
            let old_line = match self.get_line_text_for_resolver(line_no) {
                Some(t) => t,
                None => return,
            };
            let (new_line, count) = replace_in_line(&old_line, &self.search_query, &self.replace_text, false);
            if count > 0 && old_line != new_line {
                let byte_offset = self.line_index.as_ref()
                    .and_then(|idx| idx.line_to_byte_offset(self.engine.as_ref().unwrap(), line_no))
                    .unwrap_or(0);
                if let Some(ref mut doc) = self.document {
                    doc.edit_line(line_no, byte_offset, &old_line, &new_line);
                    self.viewport.apply_line_overrides(&doc.modified_lines);
                }
                for line in &mut self.viewport.lines {
                    if line.line_number == line_no {
                        line.text = new_line;
                        break;
                    }
                }
                self.status_notification = Some((
                    "Replaced 1 occurrence (Ctrl+Z to Undo)".to_string(),
                    Instant::now(),
                ));
                self.find_next();
            }
        }
    }

    pub fn replace_all_matches(&mut self) {
        if self.search_query.is_empty() || self.engine.is_none() {
            return;
        }
        if self.document.is_none() {
            if let Some(ref eng) = self.engine {
                self.document = Some(EditorDocument::new(eng.size()));
            }
        }

        let matches = self.search_matches.read().unwrap().clone();
        if matches.is_empty() {
            return;
        }

        let mut line_numbers: Vec<usize> = matches.iter().map(|m| m.line_number).collect();
        line_numbers.sort_unstable();
        line_numbers.dedup();

        let mut ops = Vec::new();
        let mut total_occurrences = 0;

        for line_no in line_numbers {
            let old_line = match self.get_line_text_for_resolver(line_no) {
                Some(t) => t,
                None => continue,
            };
            let (new_line, count) = replace_in_line(&old_line, &self.search_query, &self.replace_text, true);
            if count > 0 && old_line != new_line {
                total_occurrences += count;
                let byte_offset = self.line_index.as_ref()
                    .and_then(|idx| idx.line_to_byte_offset(self.engine.as_ref().unwrap(), line_no))
                    .unwrap_or(0);
                ops.push(crate::editor::document::EditOperation::ReplaceLine {
                    line_number: line_no,
                    byte_offset,
                    old_text: old_line,
                    new_text: new_line,
                });
            }
        }

        if !ops.is_empty() {
            let lines_count = ops.len();
            if let Some(ref mut doc) = self.document {
                doc.apply_batch(ops);
                self.viewport.apply_line_overrides(&doc.modified_lines);
            }
            for line in &mut self.viewport.lines {
                if let Some(ref doc) = self.document {
                    if let Some(mod_text) = doc.modified_lines.get(&line.line_number) {
                        line.text = mod_text.clone();
                    }
                }
            }
            self.status_notification = Some((
                format!("Replaced {} occurrences across {} lines (Atomic Undo with Ctrl+Z)", format_number(total_occurrences as u64), format_number(lines_count as u64)),
                Instant::now(),
            ));
            self.start_search();
        }
    }

    pub fn current_line(&self) -> usize {
        self.current_line
    }

    pub fn get_line_text_for_resolver(&self, line_no: usize) -> Option<String> {
        if line_no == 0 {
            return None;
        }
        if let Some(ref doc) = self.document {
            if let Some(text) = doc.modified_lines.get(&line_no) {
                return Some(text.clone());
            }
        }
        if let Some(line) = self.viewport.lines.iter().find(|l| l.line_number == line_no) {
            return Some(line.text.clone());
        }
        if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
            if let Some(offset) = index.line_to_byte_offset(engine, line_no) {
                let max_len = 4096.min((engine.size().saturating_sub(offset)) as usize);
                if let Ok(bytes) = engine.read_range(offset, max_len) {
                    let s = String::from_utf8_lossy(bytes);
                    let line_str = s.lines().next().unwrap_or("");
                    return Some(line_str.to_string());
                }
            }
        }
        None
    }

    pub fn schedule_xpath_resolution(&mut self, target_line: usize, col: Option<usize>, immediate: bool) {
        if immediate {
            self.pending_xpath = None;
            self.spawn_xpath_resolution(target_line, col);
        } else {
            self.pending_xpath = Some((target_line, col, Instant::now()));
        }
    }

    pub fn spawn_xpath_resolution(&mut self, target_line: usize, col: Option<usize>) {
        if !matches!(self.file_type, Some(FileType::Xml) | Some(FileType::Json)) {
            self.current_xpath = None;
            return;
        }
        if target_line == 0 {
            self.current_xpath = None;
            return;
        }

        let target_text = self.get_line_text_for_resolver(target_line).unwrap_or_default();
        let start_offset = self.viewport.lines.iter().find(|l| l.line_number == target_line)
            .map(|l| l.byte_offset)
            .or_else(|| {
                self.line_index.as_ref().and_then(|idx| {
                    self.engine.as_ref().and_then(|eng| idx.line_to_byte_offset(eng, target_line))
                })
            })
            .unwrap_or(0);
        let end_offset = start_offset + target_text.len() as u64;

        let engine = match self.engine.as_ref() {
            Some(e) => Arc::clone(e),
            None => {
                self.current_xpath = None;
                return;
            }
        };

        let file_type = self.file_type;
        let gen = self.xpath_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel_gen = Arc::clone(&self.xpath_generation);

        if self.xpath_rx.is_none() {
            let (tx, rx) = crossbeam_channel::unbounded();
            self.xpath_tx = Some(tx);
            self.xpath_rx = Some(rx);
        }
        let tx = self.xpath_tx.as_ref().unwrap().clone();

        std::thread::spawn(move || {
            if cancel_gen.load(Ordering::SeqCst) != gen {
                return;
            }

            let file_bytes = Some(engine.get_slice());
            let path = PathResolver::resolve_path(
                file_type,
                target_line,
                start_offset,
                end_offset,
                col,
                &target_text,
                file_bytes,
            );

            if cancel_gen.load(Ordering::SeqCst) == gen {
                if let Some(p) = path {
                    let _ = tx.send((gen, p));
                }
            }
        });

        self.last_resolved_line = target_line;
        self.last_resolved_col = col;
    }

    pub fn copy_current_xpath(&mut self, ctx: &egui::Context) {
        if let Some(ref path) = self.current_xpath {
            if !path.is_empty() {
                ctx.copy_text(path.clone());
                let label = if matches!(self.file_type, Some(FileType::Xml)) { "XPath" } else { "JSONPath" };
                self.status_notification = Some((format!("Copied {}: {}", label, path), Instant::now()));
            }
        }
    }

    pub fn start_query_search(&mut self, query_str: String) {
        // Finish ALL pending edits before starting a new query so that the
        // piece table matches what the user sees on screen.
        self.flush_all_pending_edits();

        if query_str.trim().is_empty() {
            self.query_matches.clear();
            self.query_match_idx = 0;
            self.is_query_running = false;
            return;
        }

        let engine = match self.engine.as_ref() {
            Some(e) => Arc::clone(e),
            None => return,
        };
        let line_index = self.line_index.as_ref().map(Arc::clone);

        let file_type = match self.file_type {
            Some(t) => t,
            None => return,
        };
        let edited_document = self.document.as_ref().filter(|doc| {
            doc.is_dirty() || !doc.modified_lines.is_empty()
        }).cloned();

        if let Some(ref c) = self.query_cancel {
            c.store(true, Ordering::SeqCst);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.query_cancel = Some(Arc::clone(&cancel));
        self.is_query_running = true;
        self.query_matches.clear();
        self.query_match_idx = 0;

        let (tx, rx) = crossbeam_channel::unbounded();
        self.query_rx = Some(rx);

        let gen = self.query_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel_gen = Arc::clone(&self.query_generation);

        std::thread::Builder::new()
            .name("query-evaluator".to_string())
            .spawn(move || {
                if cancel_gen.load(Ordering::SeqCst) != gen {
                    return;
                }

                let bytes = engine.get_slice();
                match file_type {
                    FileType::Xml => {
                        if let Ok(xpath) = XPathQuery::parse(&query_str) {
                            if let Some(doc) = edited_document {
                                let reader = PieceTableReader::new(&doc.piece_table, &engine);
                                let _ = StreamingQueryEngine::query_xml_streaming(reader, &xpath, cancel, tx);
                            } else {
                                let _ = StreamingQueryEngine::query_xml_streaming_indexed(
                                    Cursor::new(bytes),
                                    &xpath,
                                    line_index.as_deref(),
                                    Some(&engine),
                                    cancel,
                                    tx,
                                );
                            }
                        }
                    }
                    FileType::Json => {
                        if let Ok(jpath) = JsonPathQuery::parse(&query_str) {
                            let _ = StreamingQueryEngine::query_json_streaming(bytes, &jpath, cancel, tx);
                        }
                    }
                    _ => {}
                }
            })
            .ok();
    }

    pub fn next_query_match(&mut self) {
        if self.query_matches.is_empty() {
            return;
        }
        self.query_match_idx = if self.query_match_idx >= self.query_matches.len() {
            1
        } else {
            self.query_match_idx + 1
        };
        let line = self.query_matches[self.query_match_idx - 1].line_number;
        self.scroll_to_line_with_headroom(line, 4);
    }

    pub fn prev_query_match(&mut self) {
        if self.query_matches.is_empty() {
            return;
        }
        self.query_match_idx = if self.query_match_idx <= 1 {
            self.query_matches.len()
        } else {
            self.query_match_idx - 1
        };
        let line = self.query_matches[self.query_match_idx - 1].line_number;
        self.scroll_to_line_with_headroom(line, 4);
    }

    pub fn activate_virtual_slice_from_query(&mut self) {
        if self.active_slice.is_some() {
            self.active_slice = None;
            self.scroll_to_line(1);
            return;
        }

        if self.query_matches.is_empty() {
            return;
        }

        let mut lines: Vec<usize> = self.query_matches.iter().map(|m| m.line_number).collect();
        lines.sort_unstable();
        lines.dedup();

        let total_lines = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(0);
        let slice = VirtualSlice::new(
            format!("Query: {}", self.query_text),
            SliceSource::Query(self.query_text.clone()),
            lines,
            total_lines,
        );

        self.active_slice = Some(slice);
        self.scroll_to_line(1);
    }

    pub fn exit_virtual_slice(&mut self) {
        self.active_slice = None;
        self.scroll_to_line(1);
    }

    pub fn export_virtual_slice(&mut self, output_path: &Path) {
        let (Some(ref engine), Some(ref index), Some(ref slice)) = (&self.engine, &self.line_index, &self.active_slice) else {
            return;
        };

        if let Ok(mut out_file) = std::fs::File::create(output_path) {
            use std::io::Write;
            for &line_no in &slice.matching_lines {
                if let Some(offset) = index.line_to_byte_offset(engine, line_no) {
                    let max_len = 8192.min((engine.size() - offset) as usize);
                    if let Ok(bytes) = engine.read_range(offset, max_len) {
                        let end_pos = memchr::memchr(b'\n', bytes).unwrap_or(bytes.len());
                        let _ = out_file.write_all(&bytes[..end_pos]);
                        let _ = out_file.write_all(b"\n");
                    }
                }
            }
            let _ = out_file.flush();
            self.status_notification = Some((format!("Virtual slice exported to {:?}", output_path), Instant::now()));
        }
    }

    pub fn open_virtual_slice_in_new_tab(&mut self) {
        let (Some(ref engine), Some(ref _index), Some(ref _slice)) = (&self.engine, &self.line_index, &self.active_slice) else {
            return;
        };

        let ext = engine.path().extension().and_then(|e| e.to_str()).unwrap_or("txt");
        let stem = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("slice");
        let temp_dir = std::env::temp_dir();
        let temp_slice_path = temp_dir.join(format!("{}_slice_{}.{}", stem, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(), ext));

        self.export_virtual_slice(&temp_slice_path);
        self.open_file(&temp_slice_path);
    }

    pub fn start_field_extraction(
        &mut self,
        record_tag: String,
        fields: Vec<String>,
        delimiter: u8,
        include_headers: bool,
        output_path: Option<PathBuf>,
    ) {
        let engine = match self.engine.as_ref() {
            Some(e) => Arc::clone(e),
            None => return,
        };

        let file_type = match self.file_type {
            Some(t) => t,
            None => return,
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.field_extract_cancel = Some(Arc::clone(&cancel));
        self.field_extract_state.is_running = true;
        self.field_extract_state.status_message = None;
        self.field_extract_state.progress = None;

        let dst_path = output_path.clone().unwrap_or_else(|| {
            let temp_dir = std::env::temp_dir();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            temp_dir.join(format!("extracted_{}.csv", timestamp))
        });

        let dst_thread = dst_path.clone();
        let total_bytes = engine.size();
        let is_open_in_tab = output_path.is_none();

        let (tx, rx) = crossbeam_channel::bounded(1);
        self.field_extract_rx = Some(rx);

        let (prog_tx, prog_rx) = crossbeam_channel::unbounded();
        self.field_extract_prog_rx = Some(prog_rx);

        std::thread::Builder::new()
            .name("field-extractor".to_string())
            .spawn(move || {
                let file_res = std::fs::File::create(&dst_thread);
                let result = match file_res {
                    Ok(mut out_file) => {
                        let bytes = engine.get_slice();
                        let cursor = std::io::Cursor::new(bytes);

                        let res = match file_type {
                            FileType::Xml => StreamingFieldExtractor::extract_xml(
                                cursor,
                                &mut out_file,
                                &record_tag,
                                &fields,
                                delimiter,
                                include_headers,
                                cancel,
                                total_bytes,
                                Some(prog_tx),
                            ),
                            FileType::Json => StreamingFieldExtractor::extract_json(
                                cursor,
                                &mut out_file,
                                &fields,
                                delimiter,
                                include_headers,
                                cancel,
                                total_bytes,
                                Some(prog_tx),
                            ),
                            _ => Err("Unsupported format for field extraction".to_string()),
                        };
                        use std::io::Write;
                        let _ = out_file.flush();
                        res
                    }
                    Err(e) => Err(format!("Failed to create destination file: {}", e)),
                };
                let _ = tx.send((dst_thread, is_open_in_tab, result));
            })
            .ok();
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
        self.flush_active_line_edit();
        let (Some(ref engine), Some(ref line_index)) = (&self.engine, &self.line_index) else {
            return;
        };

        if let Some(ref c) = self.xml_validation_cancel {
            c.store(true, Ordering::SeqCst);
        }

        self.is_validating_xml = true;
        let cancel = Arc::new(AtomicBool::new(false));
        self.xml_validation_cancel = Some(Arc::clone(&cancel));
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.xml_validation_rx = Some(rx);

        let is_modified = self.document.as_ref().map_or(false, |d| {
            d.is_dirty() || !d.modified_lines.is_empty()
        });
        let doc_clone = self.document.clone();
        let engine_clone = Arc::clone(engine);
        let line_index_clone = Arc::clone(line_index);
        let file_path = engine.path().to_path_buf();

        std::thread::Builder::new()
            .name("xml-validator".to_string())
            .spawn(move || {
                let res = if let Some(doc) = doc_clone.filter(|_| is_modified) {
                    let reader = PieceTableReader::new(&doc.piece_table, &engine_clone);
                    XmlValidator::validate(reader, Some(&line_index_clone), Some(&engine_clone), cancel)
                } else {
                    match std::fs::File::open(&file_path) {
                        Ok(f) => XmlValidator::validate(f, Some(&line_index_clone), Some(&engine_clone), cancel),
                        Err(e) => XmlValidationResult::Invalid {
                            line_number: 1,
                            byte_offset: 0,
                            message: format!("Cannot open file: {}", e),
                        },
                    }
                };
                let _ = tx.send(res);
            })
            .expect("Failed to spawn XML validator thread");
    }

    pub fn normalize_xml_declaration_line(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with("<?xml") {
            return None;
        }
        let after = &trimmed[5..];
        let mut rest = after.trim_start();
        let mut had_duplicate = false;
        while rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("xml") {
            if rest.len() == 3 || rest.as_bytes()[3].is_ascii_whitespace() || rest.as_bytes()[3] == b'?' {
                had_duplicate = true;
                rest = rest[3..].trim_start();
            } else {
                break;
            }
        }
        if !had_duplicate {
            return None;
        }
        let rest_clean = rest.trim_end().trim_end_matches('>').trim_end_matches('?').trim();
        let new_decl = if rest_clean.is_empty() {
            "<?xml?>".to_string()
        } else {
            format!("<?xml {}?>", rest_clean)
        };
        Some(new_decl)
    }

    pub fn fix_malformed_xml_declaration(&mut self) -> bool {
        if self.file_type != Some(FileType::Xml) {
            return false;
        }
        let first_line_info = self.viewport.lines.iter().find(|l| l.line_number == 1).map(|l| (l.byte_offset, l.text.clone()));
        let (byte_offset, current_text) = match first_line_info {
            Some(info) => info,
            None => {
                if let (Some(ref eng), Some(ref idx)) = (&self.engine, &self.line_index) {
                    if let Some(offset) = idx.line_to_byte_offset(eng, 1) {
                        let len = (1024).min(eng.size().saturating_sub(offset) as usize);
                        if let Ok(bytes) = eng.read_range(offset, len) {
                            let s = String::from_utf8_lossy(bytes);
                            let first_line = s.lines().next().unwrap_or("").to_string();
                            (offset, first_line)
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        };

        if let Some(normalized) = Self::normalize_xml_declaration_line(&current_text) {
            if normalized != current_text {
                let old_len = current_text.as_bytes().len();
                let new_len = normalized.as_bytes().len();
                let delta = (new_len as i64) - (old_len as i64);
                if let Some(ref mut doc) = self.document {
                    doc.edit_line(1, byte_offset, &current_text, &normalized);
                    doc.modified_lines.insert(1, normalized.clone());
                }
                if delta != 0 {
                    for line in &mut self.viewport.lines {
                        if line.line_number > 1 {
                            line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
                        }
                    }
                }
                if let Some(line) = self.viewport.lines.iter_mut().find(|l| l.line_number == 1) {
                    line.text = normalized;
                }
                self.status_notification = Some(("Repaired malformed XML declaration on Line 1".to_string(), Instant::now()));
                if self.auto_save {
                    self.auto_save_timer = Some(Instant::now());
                }
                return true;
            }
        }
        false
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
        self.flush_active_line_edit();
        let Some(ref engine) = self.engine else {
            return;
        };

        if let Some(ref c) = self.json_validation_cancel {
            c.store(true, Ordering::SeqCst);
        }

        self.is_validating_json = true;
        let cancel = Arc::new(AtomicBool::new(false));
        self.json_validation_cancel = Some(Arc::clone(&cancel));
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.json_validation_rx = Some(rx);

        let is_modified = self.document.as_ref().map_or(false, |d| d.is_dirty() || !d.modified_lines.is_empty());
        let doc_clone = self.document.clone();
        let engine_clone = Arc::clone(engine);
        let file_path = engine.path().to_path_buf();

        std::thread::Builder::new()
            .name("json-validator".to_string())
            .spawn(move || {
                let res = if let Some(doc) = doc_clone.filter(|_| is_modified) {
                    let reader = PieceTableReader::new(&doc.piece_table, &engine_clone);
                    JsonValidator::validate(reader, cancel)
                } else {
                    match std::fs::File::open(&file_path) {
                        Ok(f) => JsonValidator::validate(f, cancel),
                        Err(e) => JsonValidationResult::Invalid {
                            line_number: 1,
                            byte_offset: 0,
                            message: format!("Cannot open file: {}", e),
                        },
                    }
                };
                let _ = tx.send(res);
            })
            .expect("Failed to spawn JSON validator thread");
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
        self.flush_active_line_edit();
        let Some(engine) = self.engine.as_ref().map(Arc::clone) else {
            return;
        };

        let is_modified = self.document.as_ref().map_or(false, |d| {
            d.is_dirty() || !d.modified_lines.is_empty()
        });

        let file_type = self.file_type.unwrap_or(FileType::PlainText);
        let total_bytes = if is_modified {
            self.document.as_ref().map(|d| d.piece_table.total_length()).unwrap_or(engine.size())
        } else {
            engine.size()
        };

        self.cancel_analysis();
        self.analyzer_state.reset_for_new_analysis();

        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(FormattingProgress::new(total_bytes)));
        self.analyzer_state.progress = Some(Arc::clone(&progress));

        let cancel_thread = Arc::clone(&cancel);
        let progress_thread = Arc::clone(&progress);
        let (tx, rx) = crossbeam_channel::bounded(1);

        let engine_clone = Arc::clone(&engine);
        let doc_clone = self.document.clone();
        let src_path = engine.path().to_path_buf();

        std::thread::Builder::new()
            .name("field-analyzer".to_string())
            .spawn(move || {
                let res = if let Some(doc) = doc_clone.filter(|_| is_modified) {
                    let reader = PieceTableReader::new(&doc.piece_table, &engine_clone);
                    match file_type {
                        FileType::Json => StreamingFieldAnalyzer::analyze_json(reader, total_bytes, cancel_thread, Some(progress_thread)),
                        FileType::Csv => StreamingFieldAnalyzer::analyze_csv(reader, total_bytes, b',', cancel_thread, Some(progress_thread)),
                        FileType::Xml => StreamingFieldAnalyzer::analyze_xml(reader, total_bytes, cancel_thread, Some(progress_thread)),
                        _ => StreamingFieldAnalyzer::analyze_csv(reader, total_bytes, b',', cancel_thread, Some(progress_thread)),
                    }
                } else {
                    StreamingFieldAnalyzer::analyze_file(
                        src_path,
                        file_type,
                        cancel_thread,
                        Some(progress_thread),
                    )
                };
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

    pub fn export_analysis_summary_csv(&mut self) {
        let report = match self.analyzer_state.report.as_ref() {
            Some(r) => r,
            None => return,
        };

        let default_name = if let Some(ref engine) = self.engine {
            let stem = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("fields");
            format!("{}_fields_summary.csv", stem)
        } else {
            "fields_summary.csv".to_string()
        };

        if let Some(save_path) = rfd::FileDialog::new()
            .set_title("Export Fields Summary CSV")
            .set_file_name(&default_name)
            .add_filter("CSV File", &["csv"])
            .save_file()
        {
            let mut csv_out = String::from("Field,InferredType,Occurrences,TotalRecords,PresencePct,NullCount,DistinctApprox,MinLen,MaxLen,MinValue,MaxValue\n");
            for f in &report.fields {
                let type_str = format!("{:?}", f.inferred_type);
                let row = format!(
                    "{},{},{},{},{:.2},{},{},{},{},{},{}\n",
                    StreamingFieldExtractor::escape_csv_field(&f.name, b','),
                    type_str,
                    f.total_occurrences,
                    report.total_records,
                    f.presence_pct,
                    f.null_count,
                    f.cardinality_approx,
                    f.min_len,
                    f.max_len,
                    StreamingFieldExtractor::escape_csv_field(f.min_value.as_deref().unwrap_or(""), b','),
                    StreamingFieldExtractor::escape_csv_field(f.max_value.as_deref().unwrap_or(""), b','),
                );
                csv_out.push_str(&row);
            }

            if let Err(e) = std::fs::write(&save_path, csv_out) {
                self.analyzer_state.status_msg = Some(format!("Failed to export CSV: {}", e));
            } else {
                self.analyzer_state.status_msg = Some(format!("Summary CSV saved to {:?}", save_path));
            }
        }
    }

    pub fn export_field_frequencies_csv(&mut self, field_idx: usize) {
        let report = match self.analyzer_state.report.as_ref() {
            Some(r) => r,
            None => return,
        };

        let field = match report.fields.get(field_idx) {
            Some(f) => f,
            None => return,
        };

        let safe_field_name = field.name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
        let default_name = if let Some(ref engine) = self.engine {
            let stem = engine.path().file_stem().and_then(|s| s.to_str()).unwrap_or("field");
            format!("{}_{}_frequencies.csv", stem, safe_field_name)
        } else {
            format!("{}_frequencies.csv", safe_field_name)
        };

        if let Some(save_path) = rfd::FileDialog::new()
            .set_title(&format!("Export Frequencies for '{}'", field.name))
            .set_file_name(&default_name)
            .add_filter("CSV File", &["csv"])
            .save_file()
        {
            let mut csv_out = String::from("Value,Count\n");
            for v in &field.top_values {
                let row = format!(
                    "{},{}\n",
                    StreamingFieldExtractor::escape_csv_field(&v.value, b','),
                    v.count,
                );
                csv_out.push_str(&row);
            }

            if let Err(e) = std::fs::write(&save_path, csv_out) {
                self.analyzer_state.status_msg = Some(format!("Failed to export frequencies: {}", e));
            } else {
                self.analyzer_state.status_msg = Some(format!("Frequencies saved to {:?}", save_path));
            }
        }
    }

    pub fn open_field_frequencies_in_grid(&mut self, field_idx: usize) {
        let (field_name, csv_out) = {
            let report = match self.analyzer_state.report.as_ref() {
                Some(r) => r,
                None => return,
            };

            let field = match report.fields.get(field_idx) {
                Some(f) => f,
                None => return,
            };

            let mut csv_out = String::from("Value,Count\n");
            for v in &field.top_values {
                let row = format!(
                    "{},{}\n",
                    StreamingFieldExtractor::escape_csv_field(&v.value, b','),
                    v.count,
                );
                csv_out.push_str(&row);
            }
            (field.name.clone(), csv_out)
        };

        let temp_dir = std::env::temp_dir();
        let safe_name = field_name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let temp_path = temp_dir.join(format!("{}_frequencies_{}.csv", safe_name, timestamp));

        if let Err(e) = std::fs::write(&temp_path, csv_out) {
            self.analyzer_state.status_msg = Some(format!("Failed to create temporary CSV: {}", e));
            return;
        }

        self.analyzer_state.is_open = false;
        self.open_file(&temp_path);
        self.csv_grid.is_enabled = true;
        self.status_notification = Some((format!("Opened '{}' frequencies in CSV Grid", field_name), Instant::now()));
    }

    pub fn open_sample_record_in_new_tab(&mut self) {
        let engine = match self.engine.as_ref() {
            Some(e) => e,
            None => {
                self.analyzer_state.status_msg = Some("No file open".to_string());
                return;
            }
        };

        let slice = engine.get_slice();
        if slice.is_empty() {
            self.analyzer_state.status_msg = Some("File is empty".to_string());
            return;
        }

        let file_type = self.file_type.unwrap_or_else(|| {
            let ext = engine.path().extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("xml") {
                crate::formats::FileType::Xml
            } else if ext.eq_ignore_ascii_case("json") {
                crate::formats::FileType::Json
            } else if ext.eq_ignore_ascii_case("csv") {
                crate::formats::FileType::Csv
            } else {
                crate::formats::FileType::PlainText
            }
        });

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let temp_dir = std::env::temp_dir();

        match file_type {
            crate::formats::FileType::Xml => {
                let rec_tag = self.analyzer_state.report.as_ref()
                    .and_then(|r| r.record_tag.clone());

                if let Some((tag_name, raw_snippet)) = Self::extract_sample_xml(slice, rec_tag.as_deref()) {
                    let mut beautified = Vec::new();
                    let cancel = Arc::new(AtomicBool::new(false));
                    let fmt_res = crate::formats::formatter::XmlStreamingFormatter::format(
                        std::io::Cursor::new(&raw_snippet),
                        &mut beautified,
                        crate::formats::formatter::FormatAction::Beautify { indent_size: 2, use_tabs: false },
                        cancel,
                        raw_snippet.len() as u64,
                        None,
                    );
                    let mut final_content = Vec::new();
                    if !raw_snippet.starts_with(b"<?xml") {
                        final_content.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
                    }
                    if fmt_res.is_ok() && !beautified.is_empty() {
                        final_content.extend_from_slice(&beautified);
                    } else {
                        final_content.extend_from_slice(&raw_snippet);
                    }

                    let clean_tag = tag_name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                    let temp_path = temp_dir.join(format!("sample_{}_{}.xml", clean_tag, timestamp));
                    if let Err(e) = std::fs::write(&temp_path, final_content) {
                        self.analyzer_state.status_msg = Some(format!("Failed to write sample: {}", e));
                        return;
                    }
                    self.analyzer_state.is_open = false;
                    self.open_file(&temp_path);
                    self.status_notification = Some((format!("Extracted sample <{}> snippet in new tab", tag_name), Instant::now()));
                } else {
                    self.analyzer_state.status_msg = Some("Could not find any record element in XML".to_string());
                }
            }
            crate::formats::FileType::Json => {
                if let Some(raw_snippet) = Self::extract_sample_json(slice) {
                    let mut beautified = Vec::new();
                    let cancel = Arc::new(AtomicBool::new(false));
                    let fmt_res = crate::formats::formatter::JsonStreamingFormatter::format(
                        std::io::Cursor::new(&raw_snippet),
                        &mut beautified,
                        crate::formats::formatter::FormatAction::Beautify { indent_size: 2, use_tabs: false },
                        cancel,
                        raw_snippet.len() as u64,
                        None,
                    );
                    let final_content = if fmt_res.is_ok() && !beautified.is_empty() {
                        beautified
                    } else {
                        raw_snippet
                    };

                    let temp_path = temp_dir.join(format!("sample_record_{}.json", timestamp));
                    if let Err(e) = std::fs::write(&temp_path, final_content) {
                        self.analyzer_state.status_msg = Some(format!("Failed to write sample: {}", e));
                        return;
                    }
                    self.analyzer_state.is_open = false;
                    self.open_file(&temp_path);
                    self.status_notification = Some(("Extracted sample JSON record in new tab".to_string(), Instant::now()));
                } else {
                    self.analyzer_state.status_msg = Some("Could not find any JSON record object".to_string());
                }
            }
            _ => {
                let text = String::from_utf8_lossy(slice);
                let sample_lines: Vec<&str> = text.lines().take(2).collect();
                let sample_text = sample_lines.join("\n");
                let temp_path = temp_dir.join(format!("sample_record_{}.csv", timestamp));
                if let Err(e) = std::fs::write(&temp_path, sample_text) {
                    self.analyzer_state.status_msg = Some(format!("Failed to write sample: {}", e));
                    return;
                }
                self.analyzer_state.is_open = false;
                self.open_file(&temp_path);
                self.status_notification = Some(("Extracted sample record in new tab".to_string(), Instant::now()));
            }
        }
    }

    fn extract_sample_xml(slice: &[u8], record_tag: Option<&str>) -> Option<(String, Vec<u8>)> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_reader(std::io::Cursor::new(slice));
        reader.config_mut().expand_empty_elements = false;
        reader.config_mut().trim_text(false);

        let mut buf = Vec::with_capacity(4096);
        let mut depth: usize = 0;
        let mut target_tag: Option<String> = record_tag.map(|s| s.to_string());
        let mut target_depth: Option<usize> = None;
        let mut start_pos: Option<usize> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    depth += 1;
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let should_match = if let Some(ref target) = target_tag {
                        name.eq_ignore_ascii_case(target)
                    } else if depth == 2 {
                        target_tag = Some(name.clone());
                        true
                    } else {
                        false
                    };

                    if should_match && start_pos.is_none() {
                        let end_pos = reader.buffer_position() as usize;
                        let s_pos = slice[..end_pos].iter().rposition(|&b| b == b'<')?;
                        start_pos = Some(s_pos);
                        target_depth = Some(depth);
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let should_match = if let Some(ref target) = target_tag {
                        name.eq_ignore_ascii_case(target)
                    } else if depth + 1 == 2 {
                        target_tag = Some(name.clone());
                        true
                    } else {
                        false
                    };

                    if should_match && start_pos.is_none() {
                        let end_pos = reader.buffer_position() as usize;
                        let s_pos = slice[..end_pos].iter().rposition(|&b| b == b'<')?;
                        let chosen_tag = target_tag.unwrap_or(name);
                        return Some((chosen_tag, slice[s_pos..end_pos].to_vec()));
                    }
                }
                Ok(Event::End(ref e)) => {
                    if let Some(t_depth) = target_depth {
                        if depth == t_depth {
                            let end_pos = reader.buffer_position() as usize;
                            let s_pos = start_pos?;
                            let chosen_tag = target_tag.unwrap_or_else(|| String::from_utf8_lossy(e.name().as_ref()).to_string());
                            return Some((chosen_tag, slice[s_pos..end_pos].to_vec()));
                        }
                    }
                    depth = depth.saturating_sub(1);
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        // Fallback: If no depth 2 or matching record_tag was matched, take root element
        let mut reader2 = Reader::from_reader(std::io::Cursor::new(slice));
        reader2.config_mut().expand_empty_elements = false;
        let mut buf2 = Vec::new();
        while let Ok(evt) = reader2.read_event_into(&mut buf2) {
            match evt {
                Event::Start(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let end_pos = reader2.buffer_position() as usize;
                    if let Some(s_pos) = slice[..end_pos].iter().rposition(|&b| b == b'<') {
                        let mut root_depth = 1;
                        let mut b3 = Vec::new();
                        while let Ok(ev) = reader2.read_event_into(&mut b3) {
                            match ev {
                                Event::Start(_) => root_depth += 1,
                                Event::End(_) => {
                                    root_depth -= 1;
                                    if root_depth == 0 {
                                        let final_end = reader2.buffer_position() as usize;
                                        return Some((name, slice[s_pos..final_end].to_vec()));
                                    }
                                }
                                Event::Eof => break,
                                _ => {}
                            }
                            b3.clear();
                        }
                    }
                    break;
                }
                Event::Empty(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let end_pos = reader2.buffer_position() as usize;
                    if let Some(s_pos) = slice[..end_pos].iter().rposition(|&b| b == b'<') {
                        return Some((name, slice[s_pos..end_pos].to_vec()));
                    }
                    break;
                }
                Event::Eof => break,
                _ => {}
            }
            buf2.clear();
        }

        None
    }

    fn extract_sample_json(slice: &[u8]) -> Option<Vec<u8>> {
        // Step 1: Look for '{' following '[' (i.e. first element of an array)
        let mut in_string = false;
        let mut escaped = false;
        let mut last_sig_char: Option<u8> = None;
        let mut target_start: Option<usize> = None;

        for (i, &b) in slice.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }

            if b == b'"' {
                in_string = true;
                escaped = false;
                last_sig_char = Some(b'"');
                continue;
            }

            if b.is_ascii_whitespace() {
                continue;
            }

            if b == b'{' && last_sig_char == Some(b'[') {
                target_start = Some(i);
                break;
            }

            last_sig_char = Some(b);
        }

        // Step 2: Fallback to the very first '{' if no array element was found
        let start_pos = target_start.or_else(|| {
            let mut in_str = false;
            let mut esc = false;
            for (i, &b) in slice.iter().enumerate() {
                if in_str {
                    if esc { esc = false; }
                    else if b == b'\\' { esc = true; }
                    else if b == b'"' { in_str = false; }
                    continue;
                }
                if b == b'"' { in_str = true; esc = false; continue; }
                if b == b'{' { return Some(i); }
            }
            None
        })?;

        // Step 3: Find matching '}' from start_pos
        let mut depth = 0;
        let mut in_str = false;
        let mut esc = false;
        for (i, &b) in slice[start_pos..].iter().enumerate() {
            let idx = start_pos + i;
            if in_str {
                if esc {
                    esc = false;
                } else if b == b'\\' {
                    esc = true;
                } else if b == b'"' {
                    in_str = false;
                }
                continue;
            }

            if b == b'"' {
                in_str = true;
                esc = false;
                continue;
            }

            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some(slice[start_pos..=idx].to_vec());
                }
            }
        }

        None
    }

    pub fn open_analysis_summary_in_grid(&mut self) {
        let csv_out = {
            let report = match self.analyzer_state.report.as_ref() {
                Some(r) => r,
                None => return,
            };

            let mut csv_out = String::from("Field,InferredType,Occurrences,TotalRecords,PresencePct,NullCount,DistinctApprox,MinLen,MaxLen,MinValue,MaxValue\n");
            for f in &report.fields {
                let type_str = format!("{:?}", f.inferred_type);
                let row = format!(
                    "{},{},{},{},{:.2},{},{},{},{},{},{}\n",
                    StreamingFieldExtractor::escape_csv_field(&f.name, b','),
                    type_str,
                    f.total_occurrences,
                    report.total_records,
                    f.presence_pct,
                    f.null_count,
                    f.cardinality_approx,
                    f.min_len,
                    f.max_len,
                    StreamingFieldExtractor::escape_csv_field(f.min_value.as_deref().unwrap_or(""), b','),
                    StreamingFieldExtractor::escape_csv_field(f.max_value.as_deref().unwrap_or(""), b','),
                );
                csv_out.push_str(&row);
            }
            csv_out
        };

        let temp_dir = std::env::temp_dir();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let temp_path = temp_dir.join(format!("fields_summary_{}.csv", timestamp));

        if let Err(e) = std::fs::write(&temp_path, csv_out) {
            self.analyzer_state.status_msg = Some(format!("Failed to create temporary CSV: {}", e));
            return;
        }

        self.analyzer_state.is_open = false;
        self.open_file(&temp_path);
        self.csv_grid.is_enabled = true;
        self.status_notification = Some(("Opened Fields Summary in CSV Grid".to_string(), Instant::now()));
    }

    pub fn commit_line_edit(&mut self, line_number: usize, byte_offset: u64, old_text: &str) {
        let new_text = self.edit_line_buffer.clone();
        let old_len = old_text.as_bytes().len();
        let new_len = new_text.as_bytes().len();
        let delta = (new_len as i64) - (old_len as i64);
        if let Some(ref mut doc) = self.document {
            doc.edit_line(line_number, byte_offset, old_text, &new_text);
        }
        for line in &mut self.viewport.lines {
            if line.line_number == line_number {
                line.text = new_text.clone();
            } else if line.line_number > line_number && delta != 0 {
                line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
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

    pub fn flush_active_line_edit(&mut self) {
        if let Some(line_no) = self.active_edit_line.take() {
            let text = self.edit_line_buffer.clone();
            let initial_text_opt = self.line_edit_initial_texts.remove(&line_no)
                .or_else(|| {
                    self.viewport.lines.iter().find(|l| l.line_number == line_no).map(|l| l.text.clone())
                });
            if let Some(initial_text) = initial_text_opt {
                if initial_text != text {
                    let old_len = initial_text.as_bytes().len();
                    let new_len = text.as_bytes().len();
                    let delta = (new_len as i64) - (old_len as i64);
                    let offset = self.viewport.lines.iter().find(|l| l.line_number == line_no).map(|l| l.byte_offset)
                        .or_else(|| self.line_index.as_ref().and_then(|idx| self.engine.as_ref().and_then(|eng| idx.line_to_byte_offset(eng, line_no))))
                        .unwrap_or(0);
                    if let Some(ref mut doc) = self.document {
                        doc.edit_line(line_no, offset, &initial_text, &text);
                    }
                    if let Some(vl) = self.viewport.lines.iter_mut().find(|l| l.line_number == line_no) {
                        vl.text = text.clone();
                    }
                    if delta != 0 {
                        for line in &mut self.viewport.lines {
                            if line.line_number > line_no {
                                line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Flush the active edit line AND any orphaned uncommitted edits.
    ///
    /// There are two sources of uncommitted edits:
    ///
    /// 1. `line_edit_initial_texts` entries that survived `flush_active_line_edit`
    ///    (e.g. `active_edit_line` was cleared by a multi-line drag).
    ///
    /// 2. `doc.modified_lines` entries that were set by `line_changed` but whose
    ///    `line_committed` handler saw `initial_text == current_text` because the
    ///    viewport had already been overwritten by `apply_line_overrides`. In this
    ///    case both `line_edit_initial_texts` and the piece table are untouched,
    ///    so we must read the TRUE original text from the file engine.
    pub fn flush_all_pending_edits(&mut self) {
        self.flush_active_line_edit();

        // --- Pass 1: drain any remaining entries from line_edit_initial_texts ---
        let pending: Vec<(usize, String)> = self.line_edit_initial_texts.drain().collect();
        if !pending.is_empty() {
            let mut pending = pending;
            pending.sort_by_key(|(ln, _)| *ln);
            for (line_no, initial_text) in pending {
                if let Some(ref mut doc) = self.document {
                    if let Some(modified_text) = doc.modified_lines.get(&line_no).cloned() {
                        if initial_text != modified_text {
                            let offset = self.viewport.lines.iter()
                                .find(|l| l.line_number == line_no)
                                .map(|l| l.byte_offset)
                                .or_else(|| self.line_index.as_ref().and_then(|idx|
                                    self.engine.as_ref().and_then(|eng| idx.line_to_byte_offset(eng, line_no))
                                ))
                                .unwrap_or(0);
                            let old_len = initial_text.as_bytes().len();
                            let new_len = modified_text.as_bytes().len();
                            let delta = (new_len as i64) - (old_len as i64);
                            doc.edit_line(line_no, offset, &initial_text, &modified_text);
                            if delta != 0 {
                                for line in &mut self.viewport.lines {
                                    if line.line_number > line_no {
                                        line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- Pass 2: catch modified_lines entries that never had an
        //     initial_texts entry at all (the initial_text == current_text
        //     equality problem). Compare each modified_lines entry against
        //     the ORIGINAL text read from the file engine. ---
        let (engine_ref, index_ref) = match (&self.engine, &self.line_index) {
            (Some(e), Some(i)) => (e, i),
            _ => return,
        };
        let doc = match self.document {
            Some(ref mut d) => d,
            None => return,
        };
        if doc.modified_lines.is_empty() || doc.is_dirty() {
            // If is_dirty() is true, the piece table has already been
            // updated by normal edit_line calls — nothing more to do.
            return;
        }
        // Collect the entries we need to check (piece table is still pristine).
        let to_check: Vec<(usize, String)> = doc.modified_lines.iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        let mut sorted: Vec<(usize, String)> = to_check;
        sorted.sort_by_key(|(ln, _)| *ln);
        for (line_no, modified_text) in sorted {
            // Look up the byte offset for this line.
            let offset = self.viewport.lines.iter()
                .find(|l| l.line_number == line_no)
                .map(|l| l.byte_offset)
                .or_else(|| index_ref.line_to_byte_offset(engine_ref, line_no))
                .unwrap_or(0);
            // Read the original text from the file at this offset.
            let max_read = 4096u64.min(engine_ref.size().saturating_sub(offset)) as usize;
            if max_read == 0 { continue; }
            let original_text = match engine_ref.read_range(offset, max_read) {
                Ok(bytes) => {
                    let end = memchr::memchr(b'\n', bytes).unwrap_or(bytes.len());
                    let mut line_bytes = &bytes[..end];
                    if line_bytes.ends_with(b"\r") {
                        line_bytes = &line_bytes[..line_bytes.len() - 1];
                    }
                    String::from_utf8_lossy(line_bytes).into_owned()
                }
                Err(_) => continue,
            };
            if original_text != modified_text {
                let old_len = original_text.as_bytes().len();
                let new_len = modified_text.as_bytes().len();
                let delta = (new_len as i64) - (old_len as i64);
                doc.edit_line(line_no, offset, &original_text, &modified_text);
                if delta != 0 {
                    for line in &mut self.viewport.lines {
                        if line.line_number > line_no {
                            line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
                        }
                    }
                }
            }
        }
    }

    pub fn start_save_in_place(&mut self) {
        self.start_save_in_place_internal(true);
    }

    pub fn start_save_in_place_internal(&mut self, show_modal: bool) {
        self.flush_all_pending_edits();
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
        self.show_save_modal = show_modal;
    }

    pub fn reload_active_tab_from_saved_file(&mut self, orig_path: &Path, tmp_path: &Path) -> bool {
        let saved_active_edit = self.active_edit_line;
        let saved_buffer = self.edit_line_buffer.clone();
        let saved_scroll_line = self.current_line;

        // Release file handle locks on orig_path before renaming/copying on Windows
        if let Some(active_id) = self.active_tab_id {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
                tab.engine = Arc::new(FileEngine::empty());
            }
        }
        self.engine = None;

        // Atomic replace with Windows copy fallback
        let mut save_success = true;
        if let Err(e) = std::fs::rename(tmp_path, orig_path) {
            if let Err(e2) = std::fs::copy(tmp_path, orig_path) {
                self.error_message = Some(format!("Failed to finalize save: rename ({}), copy ({})", e, e2));
                save_success = false;
            } else {
                let _ = std::fs::remove_file(tmp_path);
            }
        }

        // Reopen saved file cleanly
        match FileEngine::open(orig_path) {
            Ok(new_raw_engine) => {
                let engine = Arc::new(new_raw_engine);

                // Update active tab and app file engines seamlessly without disrupting viewport or cursor
                if let Some(active_id) = self.active_tab_id {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
                        tab.engine = Arc::clone(&engine);
                        if let Some(ref mut doc) = tab.document {
                            doc.piece_table = crate::editor::PieceTable::new(engine.size());
                            doc.mark_saved();
                            doc.modified_lines.clear();
                        }
                    }
                }

                self.engine = Some(Arc::clone(&engine));
                if let Some(ref mut doc) = self.document {
                    doc.piece_table = crate::editor::PieceTable::new(engine.size());
                    doc.mark_saved();
                    doc.modified_lines.clear();
                }

                // Re-spawn line indexer for new engine size
                let line_index = Arc::new(LineIndex::new(engine.size(), engine.detect_encoding()));
                let cancel = Arc::new(AtomicBool::new(false));
                let _ = LineIndexer::spawn(Arc::clone(&engine), Arc::clone(&line_index), cancel);
                self.line_index = Some(Arc::clone(&line_index));
                if let Some(active_id) = self.active_tab_id {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
                        tab.line_index = Arc::clone(&line_index);
                    }
                }

                // Restore active edit and scroll positions seamlessly (like VS Code)
                self.active_edit_line = saved_active_edit;
                self.edit_line_buffer = saved_buffer.clone();
                if let Some(line_no) = saved_active_edit {
                    self.line_edit_initial_texts.insert(line_no, saved_buffer);
                }
                self.current_line = saved_scroll_line;

                // Re-read lines from newly opened engine so byte offsets and line texts are 100% up-to-date
                self.scroll_to_line(saved_scroll_line);
                self.sync_active_tab_state();

                if self.xml_validation_result.is_some() && self.file_type == Some(FileType::Xml) {
                    self.validate_xml_document();
                } else if self.json_validation_result.is_some() && self.file_type == Some(FileType::Json) {
                    self.validate_json_document();
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to re-read saved file: {}", e));
            }
        }

        save_success
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
            let max_line = if let Some(ref slice) = self.active_slice {
                slice.line_count().max(1)
            } else {
                let delta = self.document.as_ref().map_or(0, |d| d.total_lines_delta);
                ((index.total_lines() as i64 + delta).max(1)) as usize
            };
            let clamped = target_line.clamp(1, max_line);
            self.current_line = clamped;
            if let Some(ref slice) = self.active_slice {
                self.viewport.load_virtual_lines(engine, index, slice, clamped, VISIBLE_LINE_BUFFER);
            } else {
                let is_dirty = self.document.as_ref().map_or(false, |d| d.is_dirty());
                if is_dirty {
                    let first_line = self.viewport.lines.first().map(|l| l.line_number).unwrap_or(0);
                    if first_line != clamped || self.viewport.lines.is_empty() {
                        if let Some(ref doc) = self.document {
                            // Compute offset for clamped line in the piece table
                            let start_offset = if clamped <= 1 {
                                0
                            } else if let Some(l) = self.viewport.lines.iter().find(|l| l.line_number == clamped) {
                                l.byte_offset
                            } else {
                                let (so, _) = Self::piece_table_line_offsets_pair(&doc.piece_table, engine, clamped, clamped);
                                so.unwrap_or(0)
                            };
                            let window_bytes = (VISIBLE_LINE_BUFFER * 512).max(65536);
                            self.viewport.load_from_piece_table(&doc.piece_table, engine, start_offset, window_bytes, clamped);
                        }
                    }
                } else {
                    self.viewport.load_lines_indexed(engine, index, clamped, VISIBLE_LINE_BUFFER);
                }
            }
            if let Some(ref doc) = self.document {
                self.viewport.apply_line_overrides(&doc.modified_lines);
            }
            self.jump_line_input = clamped.to_string();
            if let Some(offset) = self.viewport.lines.first().map(|l| l.byte_offset) {
                self.jump_offset_input = offset.to_string();
            }
        }
    }

    pub fn scroll_to_line_with_headroom(&mut self, target_line: usize, headroom: usize) {
        if let (Some(first), Some(last)) = (self.viewport.lines.first(), self.viewport.lines.last()) {
            if target_line >= first.line_number + headroom && target_line + 2 <= last.line_number {
                return;
            }
        }
        let start_line = target_line.saturating_sub(headroom).max(1);
        self.scroll_to_line(start_line);
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        if self.engine.is_some() {
            let max_line = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(usize::MAX).max(1);
            let new_line = if delta < 0 {
                self.current_line.saturating_sub((-delta) as usize).max(1)
            } else {
                self.current_line.saturating_add(delta as usize).min(max_line)
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

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (ctrl_n, ctrl_o, ctrl_w, ctrl_s, ctrl_shift_s, ctrl_z, ctrl_y, ctrl_e, ctrl_f, ctrl_h, ctrl_d, ctrl_g, ctrl_a, ctrl_c, ctrl_x, del_or_backspace, alt_up, alt_down, ctrl_shift_e, ctrl_shift_f, ctrl_shift_t, ctrl_shift_b, ctrl_shift_m, ctrl_shift_a, ctrl_shift_p, ctrl_comma, f1, f3, shift_f3, esc, zoom_in, zoom_out, zoom_reset, up, down, page_up, page_down, home, end, alt_z, tab_pressed) = ctx.input(|i| {
            let ctrl = i.modifiers.command;
            let shift = i.modifiers.shift;
            let alt = i.modifiers.alt;
            (
                ctrl && !shift && i.key_pressed(Key::N),
                ctrl && !shift && i.key_pressed(Key::O),
                ctrl && !shift && i.key_pressed(Key::W),
                ctrl && !shift && i.key_pressed(Key::S),
                ctrl && shift && i.key_pressed(Key::S),
                ctrl && !shift && i.key_pressed(Key::Z),
                ctrl && !shift && i.key_pressed(Key::Y),
                ctrl && !shift && i.key_pressed(Key::E),
                ctrl && !shift && i.key_pressed(Key::F),
                ctrl && !shift && i.key_pressed(Key::H),
                ctrl && !shift && i.key_pressed(Key::D),
                ctrl && !shift && i.key_pressed(Key::G),
                ctrl && !shift && i.key_pressed(Key::A),
                ctrl && !shift && i.key_pressed(Key::C),
                ctrl && !shift && i.key_pressed(Key::X),
                (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)),
                alt && !ctrl && i.key_pressed(Key::ArrowUp),
                alt && !ctrl && i.key_pressed(Key::ArrowDown),
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
                i.key_pressed(Key::Tab),
            )
        });

        if ctrl_n {
            self.new_blank_file();
        }
        if ctrl_a && self.engine.is_some() && self.active_edit_line.is_none() {
            let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(1);
            self.selection_anchor = Some(1);
            self.selection_anchor_col = Some(0);
            self.selection_head = Some(total);
            self.selection_head_col = Some(usize::MAX);
        }
        let has_multiline_selection = self.selection_range().map_or(false, |(s, e)| s != e);
        let has_whole_line_gutter_selection = self.active_edit_line.is_none() && self.selection_range().is_some();
        if ctrl_c && (has_multiline_selection || has_whole_line_gutter_selection) {
            self.copy_selection(ctx);
        }
        if ctrl_x && (has_multiline_selection || has_whole_line_gutter_selection) {
            self.cut_selection(ctx);
        }
        if del_or_backspace && (has_multiline_selection || has_whole_line_gutter_selection) {
            self.delete_selection();
        }
        let shift_down_arrow = ctx.input(|i| i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt && i.key_pressed(Key::ArrowDown));
        let shift_up_arrow = ctx.input(|i| i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt && i.key_pressed(Key::ArrowUp));
        if shift_down_arrow && self.engine.is_some() {
            let total = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(1);
            let anchor = self.selection_anchor.unwrap_or(self.current_line);
            let head = self.selection_head.unwrap_or(self.current_line);
            let next_head = (head + 1).min(total);
            self.selection_anchor = Some(anchor);
            self.selection_head = Some(next_head);
            self.selection_anchor_col = Some(0);
            self.selection_head_col = Some(usize::MAX);
            self.active_edit_line = None;
            ctx.memory_mut(|m| m.stop_text_input());
            self.scroll_to_line(next_head);
        }
        if shift_up_arrow && self.engine.is_some() {
            let anchor = self.selection_anchor.unwrap_or(self.current_line);
            let head = self.selection_head.unwrap_or(self.current_line);
            let prev_head = head.saturating_sub(1).max(1);
            self.selection_anchor = Some(anchor);
            self.selection_head = Some(prev_head);
            self.selection_anchor_col = Some(0);
            self.selection_head_col = Some(usize::MAX);
            self.active_edit_line = None;
            ctx.memory_mut(|m| m.stop_text_input());
            self.scroll_to_line(prev_head);
        }
        if alt_up && self.active_edit_line.is_none() && self.selection_range().is_some() {
            self.move_selection_up();
        }
        if alt_down && self.active_edit_line.is_none() && self.selection_range().is_some() {
            self.move_selection_down();
        }
        if tab_pressed && self.active_edit_line.is_none() && self.selection_range().is_some() {
            let is_shift = ctx.input(|i| i.modifiers.shift);
            if is_shift {
                self.unindent_selection();
            } else {
                self.indent_selection();
            }
        }
        let ctrl_shift_u = ctx.input_mut(|i| {
            let is_pressed = (i.modifiers.command || i.modifiers.ctrl) && i.modifiers.shift && i.key_pressed(Key::U);
            if is_pressed {
                i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::U, .. }));
            }
            is_pressed
        });
        let ctrl_u = ctx.input_mut(|i| {
            let is_pressed = (i.modifiers.command || i.modifiers.ctrl) && !i.modifiers.shift && i.key_pressed(Key::U);
            if is_pressed {
                i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::U, .. }));
            }
            is_pressed
        });
        let ctrl_shift_l = ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::L));
        if ctrl_shift_u && self.engine.is_some() {
            self.transform_selection_case(ctx, true);
        }
        if ctrl_u && self.engine.is_some() {
            self.transform_selection_case(ctx, false);
        }
        if ctrl_shift_l && self.engine.is_some() {
            self.select_all_occurrences(ctx);
        }
        if ctrl_d && self.engine.is_some() {
            self.select_next_occurrence(ctx);
        }
        if ctrl_h && self.engine.is_some() {
            ctx.input_mut(|i| {
                i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::H, .. }));
            });
            self.toggle_find_and_replace(ctx);
        }
        let ctrl_shift_c = ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::C));
        if ctrl_shift_c {
            self.copy_current_xpath(ctx);
        }
        let ctrl_shift_v = ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::V));
        if ctrl_shift_v && self.engine.is_some() {
            if self.file_type == Some(FileType::Xml) {
                self.validate_xml_document();
            } else if self.file_type == Some(FileType::Json) {
                self.validate_json_document();
            }
        }
        if ctrl_o {
            self.trigger_file_dialog();
        }
        let ctrl_u = ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::U));
        if ctrl_u {
            self.url_modal_state.open();
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
            self.show_search_bar = !self.show_search_bar;
            if self.show_search_bar {
                self.focus_search_input = true;
            }
        }
        if ctrl_g && self.engine.is_some() {
            self.show_goto_line_dialog = true;
        }
        let ctrl_shift_q = ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Q));
        if ctrl_shift_q && self.engine.is_some() {
            self.show_query_bar = !self.show_query_bar;
            if self.show_query_bar && self.query_text.is_empty() {
                if let Some(ref p) = self.current_xpath {
                    self.query_text = p.clone();
                    self.start_query_search(p.clone());
                }
            }
        }
        let shift_alt_f = ctx.input(|i| i.modifiers.shift && i.modifiers.alt && i.key_pressed(Key::F));
        if shift_alt_f && self.engine.is_some() {
            self.format_options_state.is_open = true;
            self.format_options_state.is_minify = false;
        }
        if ctrl_shift_b && self.engine.is_some() {
            self.format_options_state.is_open = true;
            self.format_options_state.is_minify = false;
        }
        if ctrl_shift_m && self.engine.is_some() {
            self.format_options_state.is_open = true;
            self.format_options_state.is_minify = true;
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
            if self.url_modal_state.is_open {
                self.url_modal_state.close();
            } else if self.analyzer_state.is_open {
                self.cancel_analysis();
                self.analyzer_state.is_open = false;
            } else if self.show_format_modal {
                self.show_format_modal = false;
            } else if self.format_options_state.is_open {
                self.format_options_state.is_open = false;
            } else if self.field_extract_state.is_open {
                self.field_extract_state.is_open = false;
            } else if self.show_replace_bar {
                self.show_replace_bar = false;
            } else if self.show_search_bar {
                self.show_search_bar = false;
            } else if self.show_query_bar {
                self.show_query_bar = false;
            } else if self.active_slice.is_some() {
                self.exit_virtual_slice();
            } else if self.show_goto_line_dialog {
                self.show_goto_line_dialog = false;
            } else if self.show_search_results {
                self.show_search_results = false;
            } else if self.active_activity_panel != ActivityPanel::None {
                self.active_activity_panel = ActivityPanel::None;
            } else if self.selection_anchor.is_some() || self.selection_head.is_some() {
                self.selection_anchor = None;
                self.selection_head = None;
            }
        }

        if zoom_in {
            self.font_size = (self.font_size + 1.0).min(32.0);
        }
        if zoom_out {
            self.font_size = (self.font_size - 1.0).max(8.0);
        }
        if zoom_reset {
            self.font_size = 16.0;
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
                self.scroll_to_line(index.total_lines());
            }
        }

        let (raw_y, smooth_y, is_ctrl) = ctx.input(|i| (i.raw_scroll_delta.y, i.smooth_scroll_delta.y, i.modifiers.command || i.modifiers.ctrl));
        let scroll_y = if raw_y.abs() > 0.1 { raw_y } else { smooth_y };
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
        let pointer_in_analyzer = self.analyzer_state.is_open
            && self
                .analyzer_state
                .window_rect
                .and_then(|r| pointer_pos.map(|pos| r.contains(pos)))
                .unwrap_or(false);
        let modal_open = self.show_goto_line_dialog
            || self.show_about_dialog
            || self.show_save_modal
            || self.show_unsaved_dialog
            || self.show_format_modal;

        let block_scroll = pointer_in_analyzer || modal_open;

        if scroll_y.abs() <= 0.1 {
            // Decay accumulator when idle so old fractions don't persist
            self.scroll_accumulator *= 0.6;
            if self.scroll_accumulator.abs() < 1.0 {
                self.scroll_accumulator = 0.0;
            }
        } else if is_ctrl && !block_scroll {
            if scroll_y > 0.0 {
                self.font_size = (self.font_size + 1.0).min(36.0);
            } else {
                self.font_size = (self.font_size - 1.0).max(8.0);
            }
            self.scroll_accumulator = 0.0;
        } else if !block_scroll {
            // If direction changes, clear existing remainder to prevent sluggish reversal
            if (self.scroll_accumulator > 0.0 && scroll_y < 0.0) || (self.scroll_accumulator < 0.0 && scroll_y > 0.0) {
                self.scroll_accumulator = 0.0;
            }

            self.scroll_accumulator += scroll_y;

            // Responsive & Balanced Smooth Scaling:
            // Standard mouse wheel notch on Windows produces ~120.0 raw units.
            // 24.0 units per line = 5 lines per standard notch (responsive, snappy, not sluggish or aggressive).
            // For smooth trackpads, 12.0 units per line provides natural, fluid finger tracking.
            let points_per_line = if raw_y.abs() > 0.1 {
                24.0_f32
            } else {
                12.0_f32
            };

            let lines = (self.scroll_accumulator / points_per_line).trunc() as isize;
            if lines != 0 {
                self.scroll_accumulator -= lines as f32 * points_per_line;
                self.scroll_lines(-lines);
                ctx.request_repaint();
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

        let is_auto_saving = self.auto_save && self.auto_save_timer.is_some();
        if is_indexing || is_searching || self.is_query_running || self.field_extract_state.is_running || self.url_modal_state.is_downloading || self.xml_tree_building || self.is_validating_xml || self.json_tree_building || self.is_validating_json || self.active_analysis.is_some() || self.active_saving.is_some() || is_auto_saving {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        // Background Auto-Save ticker (1000ms after edit pause)
        if self.auto_save {
            let doc_dirty = self.document.as_ref().map_or(false, |d| d.is_dirty() || !d.modified_lines.is_empty())
                || self.active_edit_line.is_some()
                || !self.line_edit_initial_texts.is_empty();
            if doc_dirty {
                if self.auto_save_timer.is_none() {
                    self.auto_save_timer = Some(Instant::now());
                } else if self.auto_save_timer.map_or(false, |t| t.elapsed() >= Duration::from_millis(1000)) {
                    self.auto_save_timer = None;
                    if self.active_saving.is_none() {
                        self.start_save_in_place_internal(false);
                        if self.active_saving.is_some() {
                            self.status_notification = Some(("Auto-saved document".to_string(), Instant::now()));
                        }
                    }
                }
            } else {
                self.auto_save_timer = None;
            }
        }

        // Process pending debounced XPath resolution after scrolling pauses (150ms)
        if let Some((line, col, time)) = self.pending_xpath {
            if time.elapsed() >= Duration::from_millis(150) {
                self.pending_xpath = None;
                self.spawn_xpath_resolution(line, col);
            } else {
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }

        // Receive completed XPath resolution from background thread
        if let Some(ref rx) = self.xpath_rx {
            while let Ok((gen, path)) = rx.try_recv() {
                if gen == self.xpath_generation.load(Ordering::SeqCst) {
                    self.discovered_tags.observe_path(&path);
                    self.current_xpath = Some(path);
                    ctx.request_repaint();
                }
            }
        }

        // Receive completed query matches from background query thread
        if let Some(ref rx) = self.query_rx {
            let mut new_matches = false;
            let mut is_done = false;
            loop {
                match rx.try_recv() {
                    Ok(batch) => {
                        self.query_matches.extend(batch);
                        new_matches = true;
                    }
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        // Thread finished — channel closed
                        is_done = true;
                        break;
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        break;
                    }
                }
            }
            if is_done {
                self.is_query_running = false;
                self.query_rx = None;
                if self.pending_slice_on_query_complete {
                    self.pending_slice_on_query_complete = false;
                    if !self.query_matches.is_empty() {
                        self.activate_virtual_slice_from_query();
                        self.status_notification = Some((
                            format!("Isolated {} matching records in Slice View", self.query_matches.len()),
                            Instant::now(),
                        ));
                    } else {
                        self.status_notification = Some((
                            "No matching records found for query".to_string(),
                            Instant::now(),
                        ));
                    }
                }
            }
            if new_matches {
                if self.query_match_idx == 0 && !self.query_matches.is_empty() {
                    self.query_match_idx = 1;
                    let line = self.query_matches[0].line_number;
                    self.scroll_to_line_with_headroom(line, 4);
                }
                ctx.request_repaint();
            }
            if is_done {
                ctx.request_repaint();
            }
        }
        if self.is_query_running {
            ctx.request_repaint();
        }

        // Poll background field extraction progress
        if let Some(ref rx) = self.field_extract_prog_rx {
            while let Ok(prog) = rx.try_recv() {
                self.field_extract_state.progress = Some(prog);
                ctx.request_repaint();
            }
        }

        // Poll background field extraction completion
        if let Some(ref rx) = self.field_extract_rx {
            match rx.try_recv() {
                Ok((dst_path, is_open_in_tab, result)) => {
                    self.field_extract_state.is_running = false;
                    self.field_extract_rx = None;
                    self.field_extract_prog_rx = None;
                    self.field_extract_cancel = None;

                    match result {
                        Ok(records) => {
                            if is_open_in_tab {
                                self.field_extract_state.is_open = false;
                                self.open_file(&dst_path);
                                self.csv_grid.is_enabled = true;
                                self.status_notification = Some((format!("Extracted {} records into CSV tab", records), Instant::now()));
                            } else {
                                self.field_extract_state.status_message = Some(format!("Successfully exported {} records to {:?}", records, dst_path));
                            }
                        }
                        Err(e) => {
                            self.field_extract_state.status_message = Some(format!("Extraction error: {}", e));
                        }
                    }
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.field_extract_state.is_running = false;
                    self.field_extract_rx = None;
                    self.field_extract_prog_rx = None;
                    self.field_extract_cancel = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
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

        // Poll background XML validator
        if let Some(ref rx) = self.xml_validation_rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.xml_validation_result = Some(res);
                    self.is_validating_xml = false;
                    self.xml_validation_rx = None;
                    self.xml_validation_cancel = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.is_validating_xml = false;
                    self.xml_validation_rx = None;
                    self.xml_validation_cancel = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        // Poll background JSON validator
        if let Some(ref rx) = self.json_validation_rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.json_validation_result = Some(res);
                    self.is_validating_json = false;
                    self.json_validation_rx = None;
                    self.json_validation_cancel = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.is_validating_json = false;
                    self.json_validation_rx = None;
                    self.json_validation_cancel = None;
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
                    self.show_save_modal = false;
                    if active.is_in_place {
                        let orig_path = active.target_path.clone();
                        self.reload_active_tab_from_saved_file(&orig_path, &path);
                    } else {
                        if let Some(ref mut doc) = self.document {
                            doc.mark_saved();
                        }
                        self.do_open_file(&path);
                    }

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
        let is_dirty = self.document.as_ref().map_or(false, |d| d.is_dirty() || !d.modified_lines.is_empty()) || self.active_edit_line.is_some();
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
                    .inner_margin(egui::Margin { left: 8, right: 0, top: 2, bottom: 2 }),
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
                    self.auto_save,
                    self.word_wrap,
                )
            })
            .inner;

        if let Some(action) = menu_action {
            match action {
                MenuAction::NewBlankFile => self.new_blank_file(),
                MenuAction::OpenFile => self.trigger_file_dialog(),
                MenuAction::OpenUrl => self.url_modal_state.open(),
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
                MenuAction::ToggleWordWrap => {
                    self.word_wrap = !self.word_wrap;
                    self.persist_session();
                }
                MenuAction::Find => {
                    self.show_search_bar = true;
                    self.focus_search_input = true;
                }
                MenuAction::FindAndReplace => self.toggle_find_and_replace(ctx),
                MenuAction::SelectNextOccurrence => self.select_next_occurrence(ctx),
                MenuAction::SelectAllOccurrences => self.select_all_occurrences(ctx),
                MenuAction::GoToLine => self.show_goto_line_dialog = true,
                MenuAction::ZoomIn => self.font_size = (self.font_size + 1.0).min(36.0),
                MenuAction::ZoomOut => self.font_size = (self.font_size - 1.0).max(8.0),
                MenuAction::ZoomReset => self.font_size = 16.0,
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
                MenuAction::TransformUppercase => self.transform_selection_case(ctx, true),
                MenuAction::TransformLowercase => self.transform_selection_case(ctx, false),
                MenuAction::CopyXPath => self.copy_current_xpath(ctx),
                MenuAction::SetTheme(t) => self.set_theme(t, ctx),
                MenuAction::About => self.show_about_dialog = true,
                MenuAction::ToggleAutoSave => {
                    self.auto_save = !self.auto_save;
                    self.session.auto_save = self.auto_save;
                    self.session.save();
                }
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
                        current_xpath: self.current_xpath.as_deref(),
                        word_wrap: self.word_wrap,
                    },
                )
            })
            .inner;

        if let Some(status_act) = status_action {
            match status_act {
                StatusBarAction::ResetZoom => self.font_size = 16.0,
                StatusBarAction::CopyXPath(path) => {
                    ctx.copy_text(path.clone());
                    let label = if matches!(self.file_type, Some(FileType::Xml)) { "XPath" } else { "JSONPath" };
                    self.status_notification = Some((format!("Copied {}: {}", label, path), Instant::now()));
                }
                StatusBarAction::ToggleWrap => {
                    self.word_wrap = !self.word_wrap;
                    self.persist_session();
                }
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
                    is_search_open: self.show_search_bar,
                };
                if let Some(act) = render_activity_bar(ui, &props) {
                    match act {
                        ActivityBarAction::TogglePanel(panel) => {
                            if panel == ActivityPanel::Search {
                                self.show_search_bar = !self.show_search_bar;
                                if self.show_search_bar {
                                    self.focus_search_input = true;
                                }
                            } else {
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
                    let full_path = t.path.to_str().unwrap_or(name);
                    let is_dirty = t.document.as_ref().map_or(false, |d| d.is_dirty());
                    let is_active = self.active_tab_id == Some(t.id);
                    TabInfo {
                        id: t.id,
                        name,
                        full_path,
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
                        TabBarAction::NewBlankFile => self.new_blank_file(),
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

        let show_unified_header = has_file && (is_xml || is_json) && (self.current_xpath.is_some() || self.show_query_bar);
        if show_unified_header {
            let is_dark = ctx.style().visuals.dark_mode;
            let current_xpath = self.current_xpath.clone();
            let mut query_text = self.query_text.clone();
            let file_name = self.engine.as_ref().and_then(|e| e.path().file_name().and_then(|f| f.to_str()));
            let suggestions = self.discovered_tags.suggest(&query_text, current_xpath.as_deref(), self.file_type);
            let mut show_suggestions = self.show_query_suggestions;
            let header_props = BreadcrumbBarProps {
                file_type: self.file_type,
                file_name,
                current_path: current_xpath.as_deref(),
                dark_mode: is_dark,
                is_query_open: self.show_query_bar,
                query_text: &mut query_text,
                match_count: self.query_matches.len(),
                current_match_idx: self.query_match_idx,
                is_searching: self.is_query_running,
                is_slice_active: self.active_slice.is_some(),
                suggestions: &suggestions,
                show_suggestions: &mut show_suggestions,
            };
            let header_act = egui::TopBottomPanel::top("unified_header_bar")
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| render_breadcrumb_bar(ui, header_props)).inner;
            self.query_text = query_text;
            self.show_query_suggestions = show_suggestions;

            if let Some(act) = header_act {
                match act {
                    BreadcrumbAction::CopyPath(path) => {
                        ctx.copy_text(path.clone());
                        let label = if is_xml { "XPath" } else { "JSONPath" };
                        self.status_notification = Some((format!("Copied {}: {}", label, path), Instant::now()));
                    }
                    BreadcrumbAction::ToggleQueryBar(path) => {
                        if self.show_query_bar {
                            self.show_query_bar = false;
                            self.show_query_suggestions = false;
                        } else {
                            self.show_query_bar = true;
                            self.show_query_suggestions = true;
                            self.query_text = path.clone();
                            self.start_query_search(path);
                        }
                    }
                    BreadcrumbAction::ExecuteQuery(q) => {
                        self.start_query_search(q);
                    }
                    BreadcrumbAction::NextMatch => {
                        self.next_query_match();
                    }
                    BreadcrumbAction::PrevMatch => {
                        self.prev_query_match();
                    }
                    BreadcrumbAction::ActivateSliceView => {
                        self.activate_virtual_slice_from_query();
                    }
                    BreadcrumbAction::ExtractToCsv => {
                        self.field_extract_state.is_open = true;
                        if let Some(ref m) = self.query_matches.first() {
                            let tag = m.path.rsplit('/').next().unwrap_or("item");
                            self.field_extract_state.record_tag = tag.split('[').next().unwrap_or("item").to_string();
                        }
                    }
                    BreadcrumbAction::CloseQuery => {
                        self.show_query_bar = false;
                    }
                }
            }
        }

        let mut exit_slice = false;
        let mut export_slice = false;
        let mut open_slice_tab = false;

        if let Some(ref slice) = self.active_slice {
            egui::TopBottomPanel::top("virtual_slice_banner")
                .exact_height(30.0)
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(26, 38, 54)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new("⚡ Virtual Slice:").strong().color(Color32::from_rgb(97, 175, 239)));
                        ui.label(format!("{} lines filtered from {} lines ({})", slice.line_count(), slice.total_file_lines, slice.label));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new("Exit Slice View").color(Color32::from_rgb(224, 108, 117))).clicked() {
                                exit_slice = true;
                            }
                            if ui.button("📂 Open in New Tab").clicked() {
                                open_slice_tab = true;
                            }
                            if ui.button("💾 Export Slice...").clicked() {
                                export_slice = true;
                            }
                        });
                    });
                });
        }

        if exit_slice {
            self.exit_virtual_slice();
        }
        if open_slice_tab {
            self.open_virtual_slice_in_new_tab();
        }
        if export_slice {
            if let Some(p) = rfd::FileDialog::new().set_title("Export Virtual Slice").save_file() {
                self.export_virtual_slice(&p);
            }
        }

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

        // XML Validation Banner if available
        let mut dismiss_validation = false;
        let mut trigger_revalidate_xml = false;
        let mut jump_to_xml_line = None;
        if self.is_validating_xml {
            egui::TopBottomPanel::top("xml_validating_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.colored_label(Color32::from_rgb(97, 175, 239), RichText::new("Validating XML Document in background...").strong());
                    if ui.button("Cancel").clicked() {
                        if let Some(ref c) = self.xml_validation_cancel {
                            c.store(true, Ordering::SeqCst);
                        }
                        self.is_validating_xml = false;
                    }
                });
            });
        } else if let Some(ref val_res) = self.xml_validation_result {
            egui::TopBottomPanel::top("xml_validation_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match val_res {
                        XmlValidationResult::Valid { elements_count, max_depth, elapsed_secs } => {
                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                            paint_icon(ui.painter(), icon_rect, Icon::Check, Color32::from_rgb(152, 195, 121));
                            ui.colored_label(Color32::from_rgb(152, 195, 121), RichText::new("Valid XML Document").strong());
                            ui.label(format!("({} elements, max depth: {}, verified in {:.2}s)", elements_count, max_depth, elapsed_secs));
                        }
                        XmlValidationResult::Invalid { line_number, byte_offset, message } => {
                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                            paint_icon(ui.painter(), icon_rect, Icon::Close, Color32::from_rgb(224, 108, 117));
                            ui.colored_label(Color32::from_rgb(224, 108, 117), RichText::new("XML Validation Error:").strong());
                            ui.label(RichText::new(format!("Line {}, Offset {}: {}", line_number, byte_offset, message)).color(Color32::from_rgb(235, 235, 240)));
                            if ui.button(RichText::new(format!("Jump to Line {}", line_number)).size(11.5).strong()).clicked() {
                                jump_to_xml_line = Some(*line_number);
                            }
                            if ui.button(RichText::new("Re-validate").size(11.5).strong()).clicked() {
                                trigger_revalidate_xml = true;
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            dismiss_validation = true;
                        }
                        if ui.button("Re-validate (Ctrl+Shift+V)").clicked() {
                            trigger_revalidate_xml = true;
                        }
                    });
                });
            });
        }
        if let Some(line) = jump_to_xml_line {
            self.scroll_to_line(line);
        }
        if trigger_revalidate_xml {
            self.validate_xml_document();
        }
        if dismiss_validation {
            self.xml_validation_result = None;
        }

        // JSON Validation Banner if available
        let mut dismiss_json_validation = false;
        let mut trigger_revalidate_json = false;
        let mut jump_to_json_line = None;
        if self.is_validating_json {
            egui::TopBottomPanel::top("json_validating_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.colored_label(Color32::from_rgb(229, 192, 123), RichText::new("Validating JSON Document in background...").strong());
                    if ui.button("Cancel").clicked() {
                        if let Some(ref c) = self.json_validation_cancel {
                            c.store(true, Ordering::SeqCst);
                        }
                        self.is_validating_json = false;
                    }
                });
            });
        } else if let Some(ref val_res) = self.json_validation_result {
            egui::TopBottomPanel::top("json_validation_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match val_res {
                        JsonValidationResult::Valid { objects_count, arrays_count, max_depth, elapsed_secs } => {
                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                            paint_icon(ui.painter(), icon_rect, Icon::Check, Color32::from_rgb(152, 195, 121));
                            ui.colored_label(Color32::from_rgb(152, 195, 121), RichText::new("Valid JSON Document").strong());
                            ui.label(format!("({} objects, {} arrays, max depth: {}, verified in {:.2}s)", objects_count, arrays_count, max_depth, elapsed_secs));
                        }
                        JsonValidationResult::Invalid { line_number, byte_offset, message } => {
                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                            paint_icon(ui.painter(), icon_rect, Icon::Close, Color32::from_rgb(224, 108, 117));
                            ui.colored_label(Color32::from_rgb(224, 108, 117), RichText::new("JSON Validation Error:").strong());
                            ui.label(RichText::new(format!("Line {}, Offset {}: {}", line_number, byte_offset, message)).color(Color32::from_rgb(235, 235, 240)));
                            if ui.button(RichText::new(format!("Jump to Line {}", line_number)).size(11.5).strong()).clicked() {
                                jump_to_json_line = Some(*line_number);
                            }
                            if ui.button(RichText::new("Re-validate").size(11.5).strong()).clicked() {
                                trigger_revalidate_json = true;
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            dismiss_json_validation = true;
                        }
                        if ui.button("Re-validate (Ctrl+Shift+V)").clicked() {
                            trigger_revalidate_json = true;
                        }
                    });
                });
            });
        }
        if let Some(line) = jump_to_json_line {
            self.scroll_to_line(line);
        }
        if trigger_revalidate_json {
            self.validate_json_document();
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

        // Floating Search & Replace Bar (docked dynamically to top-right of editor viewport, guaranteed below all top panels)
        if self.show_search_bar && has_file {
            let status = self.search_status.read().unwrap().clone();
            let stored_matches = self.search_matches.read().unwrap().len();
            let actual_matches = self.search_matches_count();
            let req_focus = self.focus_search_input;
            if req_focus {
                self.focus_search_input = false;
            }
            let req_rep_focus = self.focus_replace_input;
            if req_rep_focus {
                self.focus_replace_input = false;
            }

            let avail = ctx.available_rect();
            let search_pos = Pos2::new(avail.max.x - 26.0, avail.min.y + 6.0);

            egui::Area::new(egui::Id::new("floating_search_widget"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(search_pos)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(33, 37, 43))   // #21252B One Dark floating widget
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 52))) // #282C34 subtle border
                        .corner_radius(6.0)
                        .shadow(egui::Shadow {
                            offset: [0, 4],
                            blur: 12,
                            spread: 1,
                            color: Color32::from_black_alpha(140),
                        })
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            if let Some(action) = render_search_bar(
                                ui,
                                &mut self.search_query,
                                &status,
                                self.active_match_idx,
                                stored_matches,
                                actual_matches,
                                req_focus,
                                self.show_replace_bar,
                                &mut self.replace_text,
                                req_rep_focus,
                            ) {
                                match action {
                                    SearchBarAction::FindAll => {
                                        self.show_search_results = true;
                                        self.start_search();
                                    }
                                    SearchBarAction::FindNext => self.find_next(),
                                    SearchBarAction::FindPrev => self.find_prev(),
                                    SearchBarAction::Cancel => self.cancel_search(),
                                    SearchBarAction::Close => {
                                        self.show_search_bar = false;
                                        self.show_replace_bar = false;
                                    }
                                    SearchBarAction::ToggleReplace => {
                                        self.show_replace_bar = !self.show_replace_bar;
                                        if self.show_replace_bar {
                                            self.focus_replace_input = true;
                                        }
                                    }
                                    SearchBarAction::ReplaceNext => self.replace_current_match(),
                                    SearchBarAction::ReplaceAll => self.replace_all_matches(),
                                }
                            }
                        });
                });
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
                self.render_csv_grid_view(ui);
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

        // Field Extract Modal
        if let Some(act) = render_field_extract_modal(ctx, &mut self.field_extract_state, is_xml) {
            match act {
                FieldExtractModalAction::StartExportToFile { record_tag, fields, delimiter, include_headers, output_path } => {
                    self.start_field_extraction(record_tag, fields, delimiter, include_headers, Some(output_path));
                }
                FieldExtractModalAction::StartOpenInCsvGrid { record_tag, fields, delimiter, include_headers } => {
                    self.start_field_extraction(record_tag, fields, delimiter, include_headers, None);
                }
                FieldExtractModalAction::Cancel => {
                    if let Some(ref c) = self.field_extract_cancel {
                        c.store(true, Ordering::SeqCst);
                    }
                    self.field_extract_state.is_running = false;
                }
                FieldExtractModalAction::Dismiss => {
                    self.field_extract_state.is_open = false;
                }
            }
        }

        // Format Options Modal
        let file_type_name = if is_xml { "XML" } else if is_json { "JSON" } else { "Document" };
        if let Some(act) = render_format_options_modal(ctx, &mut self.format_options_state, file_type_name) {
            match act {
                FormatOptionsModalAction::Execute { indent_size, use_tabs, is_minify, target } => {
                    let action = if is_minify {
                        FormatAction::Minify
                    } else {
                        FormatAction::Beautify { indent_size, use_tabs }
                    };

                    match target {
                        FormatTarget::InPlace => {
                            self.start_formatting(action, true, None);
                        }
                        FormatTarget::NewTab => {
                            if let Some(ref eng) = self.engine {
                                let ext = eng.path().extension().and_then(|e| e.to_str()).unwrap_or("txt");
                                let stem = eng.path().file_stem().and_then(|s| s.to_str()).unwrap_or("formatted");
                                let temp_dir = std::env::temp_dir();
                                let dst = temp_dir.join(format!("{}_formatted_{}.{}", stem, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(), ext));
                                self.start_formatting(action, false, Some(dst));
                            }
                        }
                        FormatTarget::SaveAs => {
                            if let Some(ref eng) = self.engine {
                                let ext = eng.path().extension().and_then(|e| e.to_str()).unwrap_or("txt");
                                let stem = eng.path().file_stem().and_then(|s| s.to_str()).unwrap_or("formatted");
                                let suggested = format!("{}_formatted.{}", stem, ext);
                                if let Some(p) = rfd::FileDialog::new().set_file_name(&suggested).save_file() {
                                    self.start_formatting(action, false, Some(p));
                                }
                            }
                        }
                    }
                }
                FormatOptionsModalAction::Dismiss => {}
            }
        }

        // URL Downloader Modal
        let is_dark_mode = ctx.style().visuals.dark_mode;
        if let Some(act) = render_url_modal(ctx, &mut self.url_modal_state, is_dark_mode) {
            match act {
                UrlModalAction::StartDownload { url, headers } => {
                    let cancel = Arc::new(AtomicBool::new(false));
                    let (tx, rx) = crossbeam_channel::unbounded();
                    self.url_modal_state.is_downloading = true;
                    self.url_modal_state.error_message = None;
                    self.url_modal_state.progress = Some(crate::file_engine::DownloadStatus::Connecting);
                    self.url_modal_state.cancel_token = Some(Arc::clone(&cancel));
                    self.url_modal_state.rx = Some(rx);

                    crate::file_engine::UrlDownloader::spawn_download(
                        url,
                        headers,
                        None,
                        cancel,
                        tx,
                    );
                }
                UrlModalAction::CancelDownload => {
                    self.url_modal_state.close();
                }
                UrlModalAction::Dismiss => {
                    self.url_modal_state.close();
                }
                UrlModalAction::OpenDownloadedFile(path) => {
                    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("feed");
                    self.status_notification = Some((format!("Downloaded & opened: {} (Saved to %TEMP%\\ultraviewer_downloads - use Ctrl+Shift+S to save permanently)", filename), Instant::now()));
                    self.open_file(&path);
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
                AnalyzerPanelAction::OpenSampleRecord => {
                    self.open_sample_record_in_new_tab();
                }
                AnalyzerPanelAction::ExportFieldFrequencies(field_idx) => {
                    self.export_field_frequencies_csv(field_idx);
                }
                AnalyzerPanelAction::OpenFieldFrequenciesInGrid(field_idx) => {
                    self.open_field_frequencies_in_grid(field_idx);
                }
                AnalyzerPanelAction::FindIncompleteRecords(field_name) => {
                    self.flush_active_line_edit();
                    let record_tag = self.analyzer_state.report
                        .as_ref()
                        .and_then(|r| r.record_tag.clone())
                        .unwrap_or_else(|| "job".to_string());
                    let clean_field = field_name.split('/').last().unwrap_or(&field_name).to_string();
                    let query = if self.file_type == Some(FileType::Xml) {
                        format!("//{record_tag}[not({clean_field}) or {clean_field}='']")
                    } else {
                        format!("$[?(!@.{clean_field} || @.{clean_field} == '')]")
                    };
                    self.query_text = query.clone();
                    self.show_query_bar = true;
                    self.pending_slice_on_query_complete = true;
                    self.start_query_search(query);
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
            PaletteAction::NewBlankFile => self.new_blank_file(),
            PaletteAction::OpenFile => self.trigger_file_dialog(),
            PaletteAction::OpenUrl => self.url_modal_state.open(),
            PaletteAction::OpenFolder => self.trigger_folder_dialog(),
            PaletteAction::CloseActiveTab => self.close_file(),
            PaletteAction::CloseAllTabs => self.close_all_tabs(),
            PaletteAction::SaveFile => self.start_save_in_place(),
            PaletteAction::SaveFileAs => self.start_save_as(),
            PaletteAction::Find => {
                self.show_search_bar = true;
                self.focus_search_input = true;
            }
            PaletteAction::FindAndReplace => self.toggle_find_and_replace(ctx),
            PaletteAction::SelectNextOccurrence => self.select_next_occurrence(ctx),
            PaletteAction::SelectAllOccurrences => self.select_all_occurrences(ctx),
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
                self.font_size = 16.0;
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
            PaletteAction::XmlRepairDeclaration => {
                if !self.fix_malformed_xml_declaration() {
                    self.status_notification = Some(("XML declaration is already valid or not found on Line 1".to_string(), Instant::now()));
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
            PaletteAction::Undo => self.undo(),
            PaletteAction::Redo => self.redo(),
            PaletteAction::TransformUppercase => self.transform_selection_case(ctx, true),
            PaletteAction::TransformLowercase => self.transform_selection_case(ctx, false),
            PaletteAction::CopyXPath => self.copy_current_xpath(ctx),
        }
    }
fn count_exact_xml_tags(line: &str, tag_name: &str) -> (usize, usize) {
    let mut opens = 0;
    let mut closes = 0;
    let bytes = line.as_bytes();
    let n = bytes.len();
    let tag_bytes = tag_name.as_bytes();
    let tag_len = tag_bytes.len();

    let mut i = 0;
    while i < n {
        if bytes[i] == b'<' {
            if i + 1 < n && bytes[i + 1] == b'/' {
                // Closing tag </tag_name>
                let start = i + 2;
                if start + tag_len <= n && &bytes[start..start + tag_len] == tag_bytes {
                    let next_idx = start + tag_len;
                    if next_idx == n || bytes[next_idx] == b'>' || bytes[next_idx].is_ascii_whitespace() {
                        closes += 1;
                        i = next_idx;
                        continue;
                    }
                }
            } else if i + 1 < n && bytes[i + 1] != b'?' && bytes[i + 1] != b'!' {
                // Opening tag <tag_name ...>
                let start = i + 1;
                if start + tag_len <= n && &bytes[start..start + tag_len] == tag_bytes {
                    let next_idx = start + tag_len;
                    if next_idx == n || bytes[next_idx] == b'>' || bytes[next_idx] == b'/' || bytes[next_idx].is_ascii_whitespace() {
                        // Scan forward to see if this tag is self-closing (<tag_name ... />)
                        let mut is_self_closing = false;
                        let mut j = next_idx;
                        while j < n && bytes[j] != b'>' {
                            if bytes[j] == b'/' && j + 1 < n && bytes[j + 1] == b'>' {
                                is_self_closing = true;
                                break;
                            }
                            j += 1;
                        }
                        if !is_self_closing {
                            opens += 1;
                        }
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    (opens, closes)
}

fn find_folding_end(lines: &[crate::editor::ViewportLine], start_idx: usize) -> Option<usize> {
    let start_line = lines.get(start_idx)?;
    let text = start_line.text.trim();

    if !text.starts_with('<') && !text.ends_with('{') && !text.ends_with('[') {
        return None;
    }

    // XML tag folding: e.g. <item ...> ... </item>
    if text.starts_with('<') && !text.starts_with("</") && !text.starts_with("<?") && !text.starts_with("<!--") && !text.ends_with("/>") {
        let tag_name = text[1..].split(|c: char| c.is_whitespace() || c == '>').next()?;
        if !tag_name.is_empty() && !tag_name.starts_with('!') {
            let (start_opens, start_closes) = Self::count_exact_xml_tags(text, tag_name);
            if start_opens > start_closes {
                let mut depth = start_opens - start_closes;
                for i in (start_idx + 1)..lines.len() {
                    let line_text = &lines[i].text;
                    let (opens, closes) = Self::count_exact_xml_tags(line_text, tag_name);
                    depth += opens;
                    if depth <= closes {
                        return Some(lines[i].line_number);
                    }
                    depth -= closes;
                }
            }
        }
    }

    // JSON or brace/bracket folding: e.g. { or [
    if text.ends_with('{') || text.ends_with('[') {
        let open_ch = if text.ends_with('{') { '{' } else { '[' };
        let close_ch = if text.ends_with('{') { '}' } else { ']' };
        let mut depth = 1;
        for i in (start_idx + 1)..lines.len() {
            let line_text = &lines[i].text;
            for ch in line_text.chars() {
                if ch == open_ch { depth += 1; }
                else if ch == close_ch {
                    depth -= 1;
                    if depth == 0 {
                        return Some(lines[i].line_number);
                    }
                }
            }
        }
    }

    None
}

    fn render_csv_grid_view(&mut self, ui: &mut Ui) {
        let available_rect = ui.available_rect_before_wrap();
        let top_padding = 4.0;
        let content_origin = egui::pos2(available_rect.min.x, available_rect.min.y + top_padding);
        let editor_h = (available_rect.height() - top_padding).max(1.0);
        let total_w = available_rect.width();
        let ruler_w = 14.0;
        let spacing = 4.0;
        let grid_w = (total_w - ruler_w - spacing).max(100.0);

        let total_lines = self.get_total_lines();
        let row_h = 24.0_f32;
        let screen_lines = ((editor_h / row_h).floor() as usize).max(1);
        self.cached_screen_lines = screen_lines;

        let grid_rect = Rect::from_min_size(content_origin, egui::vec2(grid_w, editor_h));
        let ruler_rect = Rect::from_min_size(
            egui::pos2(content_origin.x + grid_w + spacing, content_origin.y),
            egui::vec2(ruler_w, editor_h),
        );

        ui.allocate_rect(available_rect, Sense::hover());

        // Cache headers from line 1 if empty
        if self.csv_grid.headers.is_empty() {
            if let Some(first_line) = self.viewport.lines.iter().find(|l| l.line_number == 1) {
                self.csv_grid.headers = split_delimited(&first_line.text, self.csv_grid.delimiter);
            } else if let (Some(ref engine), Some(ref index)) = (&self.engine, &self.line_index) {
                if let Some(start) = index.line_to_byte_offset(engine, 1) {
                    let len = (65536).min(engine.size().saturating_sub(start) as usize);
                    if let Ok(bytes) = engine.read_range(start, len) {
                        let s = String::from_utf8_lossy(bytes);
                        let first = s.lines().next().unwrap_or("");
                        self.csv_grid.headers = split_delimited(first, self.csv_grid.delimiter);
                    }
                }
            }
        }

        // Render CSV Grid in grid_rect
        {
            let mut grid_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(grid_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            grid_ui.set_clip_rect(grid_rect);
            render_csv_grid(&mut grid_ui, &self.viewport, self.font_size, self.csv_grid.delimiter, &self.csv_grid.headers);
        }

        // Overview Ruler on the right
        let mut scroll_target = None;
        {
            let active_match_line = if self.show_query_bar && !self.query_matches.is_empty() && self.query_match_idx > 0 {
                self.query_matches.get(self.query_match_idx - 1).map(|m| m.line_number)
            } else {
                self.active_match_idx.and_then(|idx| {
                    self.search_matches.read().unwrap().get(idx).map(|m| m.line_number)
                })
            };
            let matches_guard = self.search_matches.read().unwrap();
            let ruler_props = OverviewRulerProps {
                current_line: self.current_line,
                visible_lines_count: screen_lines.min(total_lines),
                total_lines,
                search_matches: &matches_guard,
                active_match_line,
            };
            let mut ruler_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(ruler_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let (target_line, ruler_resp) = render_overview_ruler(&mut ruler_ui, editor_h, &ruler_props);
            if ruler_resp.clicked() || ruler_resp.dragged() || ruler_resp.drag_started() {
                self.mouse_drag_start_line = None;
                self.mouse_drag_start_col = None;
            }
            if let Some(target) = target_line {
                scroll_target = Some(target);
            }
        }

        if let Some(target) = scroll_target {
            self.scroll_to_line(target);
        }
    }

    fn render_editor_viewport(&mut self, ui: &mut Ui) {
        let text_font = FontId::monospace(self.font_size);
        let line_num_font = FontId::monospace((self.font_size * 0.9).max(10.0));
        let line_num_color = Color32::from_rgb(92, 99, 112); // #5C6370 VS Code line number color

        let total_lines = self.get_total_lines();
        let file_type = self.file_type;
        let enable_highlighting = self.enable_syntax_highlighting;

        let active_match_line = if self.show_query_bar && !self.query_matches.is_empty() && self.query_match_idx > 0 {
            self.query_matches.get(self.query_match_idx - 1).map(|m| m.line_number)
        } else {
            self.active_match_idx.and_then(|idx| {
                self.search_matches.read().unwrap().get(idx).map(|m| m.line_number)
            })
        };
        let query_match_pattern = if self.show_query_bar && !self.query_matches.is_empty() && self.query_match_idx > 0 {
            let match_item = &self.query_matches[self.query_match_idx - 1];
            let tag_candidate = match_item.path.rsplit('/').next()
                .or_else(|| match_item.path.rsplit('.').next())
                .unwrap_or("");
            let clean_tag = tag_candidate.trim_start_matches('@').trim();
            if !clean_tag.is_empty() && clean_tag != "*" {
                Some(clean_tag.to_string())
            } else {
                None
            }
        } else {
            None
        };
        let search_pattern = if self.show_search_bar && !self.search_query.pattern.is_empty() {
            Some(self.search_query.pattern.clone())
        } else {
            None
        };
        let search_case = self.search_query.case_sensitive;
        let word_wrap = self.word_wrap;
        let mut scroll_target = None;
        let mut open_inspector_for: Option<(usize, u64)> = None;

        let dark_mode = ui.visuals().dark_mode;
        let mut toggle_fold_for = None;
        let mut focused_line: Option<(usize, String, Option<usize>)> = None;
        let mut line_changed: Option<(usize, u64, String)> = None;
        let mut line_committed: Option<(usize, u64, String)> = None;
        let mut split_requested: Option<(usize, usize)> = None;
        let mut merge_requested: Option<usize> = None;
        let mut new_gutter_selection: Option<(usize, usize)> = None;
        let mut clear_gutter_selection = false;

        let mut trigger_format_modal = false;
        let mut trigger_minify = false;
        let mut trigger_query_bar = false;
        let mut trigger_field_extract = false;
        let mut trigger_slice_view = false;
        let mut trigger_goto_line = false;
        let mut trigger_copy_selection = false;
        let mut trigger_delete_selection = false;
        let mut trigger_transform_upper = false;
        let mut trigger_transform_lower = false;

        // Compute folded ranges within visible lines
        let mut hidden_lines: HashSet<usize> = HashSet::new();
        let mut folding_targets: std::collections::HashMap<usize, (usize, bool)> = std::collections::HashMap::new();

        for (idx, line) in self.viewport.lines.iter().enumerate() {
            if let Some(end_line) = Self::find_folding_end(&self.viewport.lines, idx) {
                if end_line > line.line_number {
                    let is_folded = self.folded_lines.contains(&line.line_number);
                    folding_targets.insert(line.line_number, (end_line, is_folded));
                    if is_folded {
                        for h in (line.line_number + 1)..=end_line {
                            hidden_lines.insert(h);
                        }
                    }
                }
            }
        }

        let sel_range = self.selection_range();

        let primary_down = ui.input(|i| i.pointer.primary_down());
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        if !primary_down {
            if let (Some(a), Some(h)) = (self.selection_anchor, self.selection_head) {
                if a == h && !self.gutter_drag_active {
                    self.selection_anchor = None;
                    self.selection_head = None;
                    self.selection_anchor_col = None;
                    self.selection_head_col = None;
                }
            }
            self.gutter_drag_active = false;
            self.mouse_drag_start_line = None;
            self.mouse_drag_start_col = None;
        }
        let mut drag_start_detected = None;

        let mut char_sel = self.char_selection_range();
        let mut pending_focus = self.pending_focus_line;

        let available_rect = ui.available_rect_before_wrap();
        let top_padding = 4.0;
        let content_origin = egui::pos2(available_rect.min.x, available_rect.min.y + top_padding);
        let editor_h = (available_rect.height() - top_padding).max(1.0);
        let total_w = available_rect.width();
        let ruler_w = 14.0;
        let spacing = 4.0;
        let viewport_w = (total_w - ruler_w - spacing).max(100.0);

        let line_h = ui.fonts(|f| f.row_height(&text_font)).max(1.0);
        let screen_lines = ((editor_h / line_h).floor() as usize).max(1);
        self.cached_screen_lines = screen_lines;

        let viewport_rect = Rect::from_min_size(content_origin, egui::vec2(viewport_w, editor_h));
        let ruler_rect = Rect::from_min_size(
            egui::pos2(content_origin.x + viewport_w + spacing, content_origin.y),
            egui::vec2(ruler_w, editor_h),
        );

        ui.allocate_rect(available_rect, Sense::hover());

        {
            let mut vp_ui = ui.new_child(egui::UiBuilder::new().max_rect(viewport_rect).layout(egui::Layout::left_to_right(egui::Align::Min)));
            vp_ui.set_clip_rect(viewport_rect);
            let ui = &mut vp_ui;
            self.editor_viewport_rect = Some(viewport_rect);
            let max_line_str = format_number(total_lines as u64);
            let max_digits = max_line_str.len().max(5);
            let char_w = ui.fonts(|f| f.glyph_width(&line_num_font, '0')).max(7.5);
            let arrow_w = 16.0;
            let num_col_w = (max_digits as f32 * char_w).max(42.0);
            let chevron_w = 18.0;
            let gutter_w = arrow_w + num_col_w + chevron_w + 14.0;

                    if word_wrap {
                        // Word Wrap ON: Virtualized vertical line window with per-line horizontal rows
                        let wrap_width = (ui.available_width() - gutter_w - 20.0).max(100.0);
                        let top_left = ui.cursor().min;
                        let shift_down = ui.input(|i| i.modifiers.shift);
                        let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

                        ui.vertical(|ui| {
                            for line in &mut self.viewport.lines {
                                let line_no = line.line_number;
                                if hidden_lines.contains(&line_no) {
                                    continue;
                                }
                                let is_active_search = active_match_line == Some(line_no);
                                let is_truncated = line.is_truncated;
                                let line_byte_offset = line.byte_offset;

                                let is_selected = if let Some((s, e)) = sel_range {
                                    line_no >= s && line_no <= e
                                } else {
                                    false
                                };

                                ui.horizontal_top(|ui| {
                                    let line_h = ui.fonts(|f| f.row_height(&text_font));

                                    // 1. Navigation Arrow indicator (left: arrow_w = 16.0px)
                                    let (arrow_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(arrow_w, line_h),
                                        Sense::hover(),
                                    );
                                    if is_active_search && ui.is_rect_visible(arrow_rect) {
                                        ui.painter().text(
                                            arrow_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "▶",
                                            FontId::monospace(10.5),
                                            Color32::from_rgb(97, 175, 239),
                                        );
                                    }

                                    // 2. Line Number (num_col_w) - click and drag to select lines
                                    let num_str = format!("{:>width$}", format_number(line_no as u64), width = max_digits);
                                    let color = if is_active_search {
                                        Color32::from_rgb(97, 175, 239)
                                    } else {
                                        line_num_color
                                    };

                                    let (num_rect, num_resp) = ui.allocate_exact_size(
                                        egui::vec2(num_col_w, line_h),
                                        Sense::click_and_drag(),
                                    );
                                    if is_selected {
                                        ui.painter().rect_filled(
                                            num_rect,
                                            0.0,
                                            Color32::from_rgba_unmultiplied(61, 69, 86, 120),
                                        );
                                    }
                                    if ui.is_rect_visible(num_rect) {
                                        ui.painter().text(
                                            Pos2::new(num_rect.right() - 2.0, num_rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            &num_str,
                                            line_num_font.clone(),
                                            color,
                                        );
                                    }

                                    // 3. Chevron (right: 18.0px) - directly beside the code guide
                                    let (ch_rect, ch_resp) = ui.allocate_exact_size(
                                        egui::vec2(chevron_w, line_h),
                                        Sense::click(),
                                    );
                                    if let Some((_end, is_folded)) = folding_targets.get(&line_no) {
                                        let ch_hovered = ch_resp.hovered();
                                        let ch_icon = if *is_folded { Icon::ChevronRight } else { Icon::ChevronDown };
                                        let ch_color = if ch_hovered {
                                            Color32::from_rgb(230, 238, 250)
                                        } else {
                                            Color32::from_rgb(140, 155, 175)
                                        };
                                        if ch_hovered {
                                            ui.painter().rect_filled(ch_rect.shrink(1.0), 3.0, Color32::from_rgb(38, 44, 54));
                                        }
                                        paint_icon(
                                            ui.painter(),
                                            Rect::from_center_size(ch_rect.center(), Vec2::splat(11.0)),
                                            ch_icon,
                                            ch_color,
                                        );
                                        if ch_resp.on_hover_text(if *is_folded { "Unfold Block" } else { "Fold Block" }).clicked() {
                                            toggle_fold_for = Some(line_no);
                                        }
                                    }

                                    ui.add_space(4.0);

                                    // Native Line Editor (direct cursor positioning, word selection, and real-time editing)
                                    let edit_id = egui::Id::new("line_editor").with(line_no);
                                    let mut line_text = line.text.clone();

                                    if pending_focus.map_or(false, |(l, _)| l == line_no) {
                                        let target_col = pending_focus.unwrap().1;
                                        ui.ctx().memory_mut(|m| m.request_focus(edit_id));
                                        let mut state = egui::text_edit::TextEditState::default();
                                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                                            egui::text::CCursor::new(target_col),
                                        )));
                                        state.store(ui.ctx(), edit_id);
                                        pending_focus = None;
                                    }

                                    let line_sel = compute_line_selection(char_sel, line_no, &line.text);
                                    let target_wrap_w = wrap_width;
                                    let line_pattern = if self.show_query_bar {
                                        if is_active_search { query_match_pattern.as_deref() } else { None }
                                    } else {
                                        search_pattern.as_deref()
                                    };
                                    let mut layouter = |ui: &Ui, text: &str, _w: f32| {
                                        let job = build_line_layout_job(
                                            text,
                                            file_type,
                                            enable_highlighting,
                                            line_pattern,
                                            search_case,
                                            is_active_search,
                                            text_font.clone(),
                                            dark_mode,
                                            target_wrap_w,
                                            line_sel,
                                        );
                                        ui.fonts(|f| f.layout_job(job))
                                    };

                                    let te = egui::TextEdit::multiline(&mut line_text)
                                        .id(edit_id)
                                        .font(text_font.clone())
                                        .frame(false)
                                        .margin(egui::Margin::ZERO)
                                        .desired_width(target_wrap_w)
                                        .desired_rows(1)
                                        .layouter(&mut layouter);

                                    let resp = ui.add(te);
                                    if is_active_search {
                                        ui.painter().rect_filled(
                                            resp.rect,
                                            2.0,
                                            Color32::from_rgba_unmultiplied(97, 175, 239, 25),
                                        );
                                        ui.painter().rect_stroke(
                                            resp.rect,
                                            2.0,
                                            egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(97, 175, 239, 90)),
                                            egui::StrokeKind::Inside,
                                        );
                                        let accent_rect = Rect::from_min_size(resp.rect.min, Vec2::new(3.0, resp.rect.height()));
                                        ui.painter().rect_filled(accent_rect, 1.0, Color32::from_rgb(97, 175, 239));
                                    }

                                    let row_top = num_rect.min.y.min(resp.rect.min.y);
                                    let row_bot = num_rect.max.y.max(resp.rect.max.y);

                                    let pointer_in_row = pointer_pos.map_or(false, |p| p.y >= row_top && p.y <= row_bot);
                                    let char_w = ui.fonts(|f| f.glyph_width(&text_font, ' '));
                                    let line_char_count = line.text.chars().count();
                                    let cur_col = pointer_pos.map_or(0, |p| {
                                        let col = ((p.x - resp.rect.min.x).max(0.0) / char_w).round() as usize;
                                        col.min(line_char_count)
                                    });

                                    let pointer_in_text = pointer_pos.map_or(false, |p| resp.rect.contains(p));

                                    // 1. Mouse press inside text row
                                    if primary_pressed && pointer_in_text && !num_resp.hovered() && !self.gutter_drag_active {
                                        self.mouse_drag_start_line = Some(line_no);
                                        self.mouse_drag_start_col = Some(cur_col);
                                        if !shift_down {
                                            self.selection_anchor = None;
                                            self.selection_head = None;
                                            self.selection_anchor_col = None;
                                            self.selection_head_col = None;
                                            clear_gutter_selection = true;
                                        }
                                    }

                                    // 2. Mouse drag across lines
                                    if primary_down && pointer_in_row && self.mouse_drag_start_line.is_some() && !self.gutter_drag_active {
                                        if let (Some(start_l), Some(start_c)) = (self.mouse_drag_start_line, self.mouse_drag_start_col) {
                                            if start_l != line_no {
                                                self.selection_anchor = Some(start_l);
                                                self.selection_anchor_col = Some(start_c);
                                                self.selection_head = Some(line_no);
                                                self.selection_head_col = Some(cur_col);
                                                char_sel = if (start_l, start_c) <= (line_no, cur_col) {
                                                    Some(((start_l, start_c), (line_no, cur_col)))
                                                } else {
                                                    Some(((line_no, cur_col), (start_l, start_c)))
                                                };
                                                self.active_edit_line = None;
                                                focused_line = None;
                                                ui.ctx().memory_mut(|m| m.stop_text_input());
                                                ui.ctx().request_repaint();
                                            }
                                        }
                                    }

                                    // 3. Gutter interaction: click & drag down the line numbers column selects lines
                                    if num_resp.drag_started() || (num_resp.hovered() && primary_down && !self.gutter_drag_active && self.mouse_drag_start_line.is_none()) {
                                        drag_start_detected = Some(line_no);
                                        new_gutter_selection = Some((line_no, line_no));
                                        clear_gutter_selection = false;
                                    }

                                    if num_resp.clicked() && shift_down {
                                        let anchor = self.selection_anchor.unwrap_or(self.current_line);
                                        new_gutter_selection = Some((anchor, line_no));
                                        clear_gutter_selection = false;
                                    } else if num_resp.clicked() && !shift_down {
                                        let just_finished_drag = self.selection_anchor != self.selection_head && self.selection_anchor.is_some();
                                        if !just_finished_drag {
                                            new_gutter_selection = Some((line_no, line_no));
                                            clear_gutter_selection = false;
                                        }
                                    }

                                    // Multi-line cursor selection: when dragging along gutter (line numbers column)
                                    if self.gutter_drag_active && primary_down && pointer_pos.map_or(false, |p| p.y >= num_rect.min.y && p.y <= num_rect.max.y) {
                                        if let Some(start_line) = self.mouse_drag_start_line {
                                            new_gutter_selection = Some((start_line, line_no));
                                            clear_gutter_selection = false;
                                        }
                                    }

                                    let is_multiline = self.selection_anchor.is_some()
                                        && self.selection_head.is_some()
                                        && self.selection_anchor != self.selection_head;

                                    if !self.gutter_drag_active && !is_multiline {
                                        // Text editor interaction: pure in-line text editing, word selection, and typing
                                        if (resp.gained_focus() || resp.clicked()) && !shift_down {
                                            self.selection_anchor = None;
                                            self.selection_head = None;
                                            self.selection_anchor_col = None;
                                            self.selection_head_col = None;
                                            clear_gutter_selection = true;
                                            let col = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                                                .and_then(|state| state.cursor.char_range())
                                                .map(|r| r.primary.index);
                                            focused_line = Some((line_no, line.text.clone(), col));
                                        } else if resp.clicked() && shift_down {
                                            let anchor = self.selection_anchor.unwrap_or(self.current_line);
                                            new_gutter_selection = Some((anchor, line_no));
                                            clear_gutter_selection = false;
                                        }
                                    }

                                    let has_newline = line_text.contains('\n');
                                    let enter_pressed = ui.input(|i| i.key_pressed(Key::Enter)) || has_newline;
                                    if has_newline {
                                        line_text.retain(|c| c != '\n' && c != '\r');
                                    }
                                    let backspace_pressed = ui.input(|i| i.key_pressed(Key::Backspace));
                                    let delete_pressed = ui.input(|i| i.key_pressed(Key::Delete));
                                    let (current_col, has_char_selection) = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                                        .and_then(|state| state.cursor.char_range())
                                        .map(|r| (r.primary.index, r.primary.index != r.secondary.index))
                                        .unwrap_or((line_text.chars().count(), false));

                                    if self.active_edit_line == Some(line_no) && enter_pressed {
                                        split_requested = Some((line_no, current_col));
                                    // TextEdit receives keyboard events first. A deletion that
                                    // changed this line (including deleting a selected word or
                                    // all of its text) must stay in this line; it is not a
                                    // request to merge with a neighbour.
                                    } else if resp.changed() {
                                        line.text = line_text.clone();
                                        line_changed = Some((line_no, line_byte_offset, line.text.clone()));
                                    } else if !has_char_selection && self.active_edit_line == Some(line_no) && backspace_pressed && current_col == 0 && line_no > 1 {
                                        merge_requested = Some(line_no);
                                    } else if !has_char_selection && self.active_edit_line == Some(line_no) && delete_pressed && current_col >= line_text.chars().count() {
                                        let max_lines = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(usize::MAX);
                                        if line_no < max_lines {
                                            merge_requested = Some(line_no + 1);
                                        }
                                    }

                                    if resp.lost_focus() {
                                        line_committed = Some((line_no, line_byte_offset, line.text.clone()));
                                    }

                                    resp.context_menu(|ui| {
                                        ui.set_min_width(200.0);
                                        if sel_range.is_some() {
                                            if ui.add(egui::Button::new("Copy Selected Lines").shortcut_text("Ctrl+C")).clicked() {
                                                trigger_copy_selection = true;
                                                ui.close_menu();
                                            }
                                            if ui.add(egui::Button::new("Delete Selected Lines").shortcut_text("Del")).clicked() {
                                                trigger_delete_selection = true;
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                        }
                                        if ui.add(egui::Button::new("Format Document").shortcut_text("Shift+Alt+F")).clicked() {
                                            trigger_format_modal = true;
                                            ui.close_menu();
                                        }
                                        if ui.add(egui::Button::new("Minify Document").shortcut_text("Ctrl+Shift+M")).clicked() {
                                            trigger_minify = true;
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui.add(egui::Button::new("Transform to Uppercase").shortcut_text("Ctrl+Shift+U")).clicked() {
                                            trigger_transform_upper = true;
                                            ui.close_menu();
                                        }
                                        if ui.add(egui::Button::new("Transform to Lowercase").shortcut_text("Ctrl+U")).clicked() {
                                            trigger_transform_lower = true;
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui.add(egui::Button::new("XPath / JSONPath Query...").shortcut_text("Ctrl+Shift+Q")).clicked() {
                                            trigger_query_bar = true;
                                            ui.close_menu();
                                        }
                                        if ui.add(egui::Button::new("Extract Fields to CSV...")).clicked() {
                                            trigger_field_extract = true;
                                            ui.close_menu();
                                        }
                                        if ui.add(egui::Button::new("Filter to Virtual Slice View")).clicked() {
                                            trigger_slice_view = true;
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui.add(egui::Button::new("Go to Line...").shortcut_text("Ctrl+G")).clicked() {
                                            trigger_goto_line = true;
                                            ui.close_menu();
                                        }
                                    });

                                    // Folded indicator pill
                                    if let Some((end_line, true)) = folding_targets.get(&line_no) {
                                        let count = end_line.saturating_sub(line_no);
                                        let pill = egui::Button::new(
                                            RichText::new(format!("... [{} lines folded]", count))
                                                .size(10.0)
                                                .color(Color32::from_rgb(97, 175, 239))
                                        ).frame(false);
                                        if ui.add(pill).on_hover_text("Click to expand").clicked() {
                                            toggle_fold_for = Some(line_no);
                                        }
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
                        });

                        // Draw clean vertical separator line between gutter and text
                        let gutter_divider_x = top_left.x + gutter_w;
                        let bottom_y = ui.cursor().min.y;
                        ui.painter().vline(
                            gutter_divider_x,
                            top_left.y..=bottom_y,
                            egui::Stroke::new(1.0_f32, Color32::from_rgb(45, 51, 61)),
                        );
                    } else {
                        // Word Wrap OFF: Horizontal scrolling with pinned line numbers column
                        let shift_down = ui.input(|i| i.modifiers.shift);
                        let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

                        ui.style_mut().always_scroll_the_only_direction = false;
                        ui.horizontal(|ui| {
                            // Pinned line numbers column on left
                            ui.vertical(|ui| {
                                ui.set_width(gutter_w);
                                let line_h = ui.fonts(|f| f.row_height(&text_font));

                                for line in &self.viewport.lines {
                                    let line_no = line.line_number;
                                    if hidden_lines.contains(&line_no) {
                                        continue;
                                    }
                                    let is_selected = if let Some((s, e)) = sel_range {
                                        line_no >= s && line_no <= e
                                    } else {
                                        false
                                    };
                                    let is_active = active_match_line == Some(line_no);
                                    let num_str = format!("{:>width$}", format_number(line_no as u64), width = max_digits);
                                    let color = if is_active {
                                        Color32::from_rgb(97, 175, 239)
                                    } else {
                                        line_num_color
                                    };

                                    ui.horizontal(|ui| {
                                        // 1. Navigation Arrow indicator (left: arrow_w = 16.0px)
                                        let (arrow_rect, _) = ui.allocate_exact_size(
                                            egui::vec2(arrow_w, line_h),
                                            Sense::hover(),
                                        );
                                        if is_active && ui.is_rect_visible(arrow_rect) {
                                            ui.painter().text(
                                                arrow_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "▶",
                                                FontId::monospace(10.5),
                                                Color32::from_rgb(97, 175, 239),
                                            );
                                        }

                                        // 2. Line Number (num_col_w)
                                        let (num_rect, num_resp) = ui.allocate_exact_size(
                                            egui::vec2(num_col_w, line_h),
                                            Sense::click_and_drag(),
                                        );
                                        if is_selected {
                                            ui.painter().rect_filled(
                                                num_rect,
                                                0.0,
                                                Color32::from_rgba_unmultiplied(61, 69, 86, 120),
                                            );
                                        }
                                        if ui.is_rect_visible(num_rect) {
                                            ui.painter().text(
                                                Pos2::new(num_rect.right() - 2.0, num_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                &num_str,
                                                line_num_font.clone(),
                                                color,
                                            );
                                        }

                                        if num_resp.drag_started() || (num_resp.hovered() && primary_down && !self.gutter_drag_active) {
                                            drag_start_detected = Some(line_no);
                                            new_gutter_selection = Some((line_no, line_no));
                                            clear_gutter_selection = false;
                                        }

                                        if num_resp.clicked() && shift_down {
                                            let anchor = self.selection_anchor.unwrap_or(self.current_line);
                                            new_gutter_selection = Some((anchor, line_no));
                                            clear_gutter_selection = false;
                                        } else if num_resp.clicked() && !shift_down {
                                            let just_finished_drag = self.selection_anchor != self.selection_head && self.selection_anchor.is_some();
                                            if !just_finished_drag {
                                                new_gutter_selection = Some((line_no, line_no));
                                                clear_gutter_selection = false;
                                            }
                                        }

                                        if self.gutter_drag_active && primary_down && pointer_pos.map_or(false, |p| p.y >= num_rect.min.y && p.y <= num_rect.max.y) {
                                            if let Some(start_line) = self.mouse_drag_start_line {
                                                new_gutter_selection = Some((start_line, line_no));
                                                clear_gutter_selection = false;
                                            }
                                        }

                                        // 3. Chevron (right: 18.0px)
                                        let (ch_rect, ch_resp) = ui.allocate_exact_size(
                                            egui::vec2(chevron_w, line_h),
                                            Sense::click(),
                                        );
                                        if let Some((_end, is_folded)) = folding_targets.get(&line_no) {
                                            let ch_hovered = ch_resp.hovered();
                                            let ch_icon = if *is_folded { Icon::ChevronRight } else { Icon::ChevronDown };
                                            let ch_color = if ch_hovered {
                                                Color32::from_rgb(230, 238, 250)
                                            } else {
                                                Color32::from_rgb(140, 155, 175)
                                            };
                                            if ch_hovered {
                                                ui.painter().rect_filled(ch_rect.shrink(1.0), 3.0, Color32::from_rgb(38, 44, 54));
                                            }
                                            paint_icon(
                                                ui.painter(),
                                                Rect::from_center_size(ch_rect.center(), Vec2::splat(11.0)),
                                                ch_icon,
                                                ch_color,
                                            );
                                            if ch_resp.on_hover_text(if *is_folded { "Unfold Block" } else { "Fold Block" }).clicked() {
                                                toggle_fold_for = Some(line_no);
                                            }
                                        }
                                    });
                                }
                            });

                            // Crisp vertical divider line between pinned gutter and content
                            let (sep_rect, _) = ui.allocate_exact_size(egui::vec2(1.0, ui.available_height()), Sense::hover());
                            ui.painter().rect_filled(sep_rect, 0.0, Color32::from_rgb(45, 51, 61));

                                    // Content column (horizontally scrollable, while gutter remains pinned)
                                    ScrollArea::horizontal()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                        for line in &mut self.viewport.lines {
                                            let line_no = line.line_number;
                                            if hidden_lines.contains(&line_no) {
                                                continue;
                                            }
                                            let is_active_search = active_match_line == Some(line_no);
                                            let is_truncated = line.is_truncated;
                                            let line_byte_offset = line.byte_offset;

                                            ui.horizontal(|ui| {
                                                let edit_id = egui::Id::new("line_editor").with(line_no);
                                                let mut line_text = line.text.clone();

                                                    if pending_focus.map_or(false, |(l, _)| l == line_no) {
                                                        let target_col = pending_focus.unwrap().1;
                                                        ui.ctx().memory_mut(|m| m.request_focus(edit_id));
                                                        let mut state = egui::text_edit::TextEditState::default();
                                                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                                                            egui::text::CCursor::new(target_col),
                                                        )));
                                                        state.store(ui.ctx(), edit_id);
                                                        pending_focus = None;
                                                    }

                                                    let line_sel = compute_line_selection(char_sel, line_no, &line.text);
                                                    let line_pattern = if self.show_query_bar {
                                                        if is_active_search { query_match_pattern.as_deref() } else { None }
                                                    } else {
                                                        search_pattern.as_deref()
                                                    };
                                                    let mut layouter = |ui: &Ui, text: &str, _wrap_width: f32| {
                                                        let job = build_line_layout_job(
                                                            text,
                                                            file_type,
                                                            enable_highlighting,
                                                            line_pattern,
                                                            search_case,
                                                            is_active_search,
                                                            text_font.clone(),
                                                            dark_mode,
                                                            f32::INFINITY,
                                                            line_sel,
                                                        );
                                                        ui.fonts(|f| f.layout_job(job))
                                                    };

                                                let char_w = ui.fonts(|f| f.glyph_width(&text_font, ' '));
                                                let line_w = (line.text.chars().count() as f32 + 5.0) * char_w;
                                                let desired_w = line_w.max(ui.available_width());

                                                let te = egui::TextEdit::multiline(&mut line_text)
                                                    .id(edit_id)
                                                    .font(text_font.clone())
                                                    .frame(false)
                                                    .margin(egui::Margin::ZERO)
                                                    .desired_width(desired_w)
                                                    .desired_rows(1)
                                                    .layouter(&mut layouter);

                                                let resp = ui.add(te);
                                                if is_active_search {
                                                    ui.painter().rect_filled(
                                                        resp.rect,
                                                        2.0,
                                                        Color32::from_rgba_unmultiplied(97, 175, 239, 25),
                                                    );
                                                    ui.painter().rect_stroke(
                                                        resp.rect,
                                                        2.0,
                                                        egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(97, 175, 239, 90)),
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    let accent_rect = Rect::from_min_size(resp.rect.min, Vec2::new(3.0, resp.rect.height()));
                                                    ui.painter().rect_filled(accent_rect, 1.0, Color32::from_rgb(97, 175, 239));
                                                }

                                                let pointer_in_row = pointer_pos.map_or(false, |p| p.y >= resp.rect.min.y && p.y <= resp.rect.max.y);
                                                let line_char_count = line.text.chars().count();
                                                let cur_col = pointer_pos.map_or(0, |p| {
                                                    let col = ((p.x - resp.rect.min.x).max(0.0) / char_w).round() as usize;
                                                    col.min(line_char_count)
                                                });

                                                let pointer_in_text = pointer_pos.map_or(false, |p| resp.rect.contains(p));

                                                // 1. Mouse press inside text row
                                                if primary_pressed && pointer_in_text && !self.gutter_drag_active {
                                                    self.mouse_drag_start_line = Some(line_no);
                                                    self.mouse_drag_start_col = Some(cur_col);
                                                    if !shift_down {
                                                        self.selection_anchor = None;
                                                        self.selection_head = None;
                                                        self.selection_anchor_col = None;
                                                        self.selection_head_col = None;
                                                        clear_gutter_selection = true;
                                                    }
                                                }

                                                // 2. Mouse drag across lines
                                                if primary_down && pointer_in_row && self.mouse_drag_start_line.is_some() && !self.gutter_drag_active {
                                                    if let (Some(start_l), Some(start_c)) = (self.mouse_drag_start_line, self.mouse_drag_start_col) {
                                                        if start_l != line_no {
                                                            self.selection_anchor = Some(start_l);
                                                            self.selection_anchor_col = Some(start_c);
                                                            self.selection_head = Some(line_no);
                                                            self.selection_head_col = Some(cur_col);
                                                            char_sel = if (start_l, start_c) <= (line_no, cur_col) {
                                                                Some(((start_l, start_c), (line_no, cur_col)))
                                                            } else {
                                                                Some(((line_no, cur_col), (start_l, start_c)))
                                                            };
                                                            self.active_edit_line = None;
                                                            focused_line = None;
                                                            ui.ctx().memory_mut(|m| m.stop_text_input());
                                                            ui.ctx().request_repaint();
                                                        }
                                                    }
                                                }

                                                let is_multiline = self.selection_anchor.is_some()
                                                    && self.selection_head.is_some()
                                                    && self.selection_anchor != self.selection_head;

                                                if !self.gutter_drag_active && !is_multiline {
                                                    // Text editor interaction: pure in-line text editing, word selection, and typing
                                                    if (resp.gained_focus() || resp.clicked()) && !shift_down {
                                                        self.selection_anchor = None;
                                                        self.selection_head = None;
                                                        self.selection_anchor_col = None;
                                                        self.selection_head_col = None;
                                                        clear_gutter_selection = true;
                                                        let col = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                                                            .and_then(|state| state.cursor.char_range())
                                                            .map(|r| r.primary.index);
                                                        focused_line = Some((line_no, line.text.clone(), col));
                                                    } else if resp.clicked() && shift_down {
                                                        let anchor = self.selection_anchor.unwrap_or(self.current_line);
                                                        new_gutter_selection = Some((anchor, line_no));
                                                        clear_gutter_selection = false;
                                                    }
                                                }

                                                let has_newline = line_text.contains('\n');
                                                let enter_pressed = ui.input(|i| i.key_pressed(Key::Enter)) || has_newline;
                                                let backspace_pressed = ui.input(|i| i.key_pressed(Key::Backspace));
                                                let delete_pressed = ui.input(|i| i.key_pressed(Key::Delete));
                                                let (current_col, has_char_selection) = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                                                    .and_then(|state| state.cursor.char_range())
                                                    .map(|r| (r.primary.index, r.primary.index != r.secondary.index))
                                                    .unwrap_or((line_text.chars().count(), false));

                                                if has_newline {
                                                    line_text.retain(|c| c != '\n' && c != '\r');
                                                }

                                                if self.active_edit_line == Some(line_no) && enter_pressed {
                                                    split_requested = Some((line_no, current_col));
                                                // Let TextEdit handle an ordinary deletion first.
                                                // In particular, an emptied selected line should
                                                // remain an empty line instead of swallowing its
                                                // neighbour.
                                                } else if resp.changed() {
                                                    line.text = line_text.clone();
                                                    line_changed = Some((line_no, line_byte_offset, line.text.clone()));
                                                } else if !has_char_selection && self.active_edit_line == Some(line_no) && backspace_pressed && current_col == 0 && line_no > 1 {
                                                    merge_requested = Some(line_no);
                                                } else if !has_char_selection && self.active_edit_line == Some(line_no) && delete_pressed && current_col >= line_text.chars().count() {
                                                    let max_lines = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(usize::MAX);
                                                    if line_no < max_lines {
                                                        merge_requested = Some(line_no + 1);
                                                    }
                                                }

                                                if resp.lost_focus() {
                                                    line_committed = Some((line_no, line_byte_offset, line.text.clone()));
                                                }

                                                resp.context_menu(|ui| {
                                                    ui.set_min_width(200.0);
                                                    if sel_range.is_some() {
                                                        if ui.add(egui::Button::new("Copy Selected Lines").shortcut_text("Ctrl+C")).clicked() {
                                                            trigger_copy_selection = true;
                                                            ui.close_menu();
                                                        }
                                                        if ui.add(egui::Button::new("Delete Selected Lines").shortcut_text("Del")).clicked() {
                                                            trigger_delete_selection = true;
                                                            ui.close_menu();
                                                        }
                                                        ui.separator();
                                                    }
                                                    if ui.add(egui::Button::new("Format Document").shortcut_text("Shift+Alt+F")).clicked() {
                                                        trigger_format_modal = true;
                                                        ui.close_menu();
                                                    }
                                                    if ui.add(egui::Button::new("Minify Document").shortcut_text("Ctrl+Shift+M")).clicked() {
                                                        trigger_minify = true;
                                                        ui.close_menu();
                                                    }
                                                    ui.separator();
                                                    if ui.add(egui::Button::new("Transform to Uppercase").shortcut_text("Ctrl+Shift+U")).clicked() {
                                                        trigger_transform_upper = true;
                                                        ui.close_menu();
                                                    }
                                                    if ui.add(egui::Button::new("Transform to Lowercase").shortcut_text("Ctrl+U")).clicked() {
                                                        trigger_transform_lower = true;
                                                        ui.close_menu();
                                                    }
                                                    ui.separator();
                                                    if ui.add(egui::Button::new("XPath / JSONPath Query...").shortcut_text("Ctrl+Shift+Q")).clicked() {
                                                        trigger_query_bar = true;
                                                        ui.close_menu();
                                                    }
                                                    if ui.add(egui::Button::new("Extract Fields to CSV...")).clicked() {
                                                        trigger_field_extract = true;
                                                        ui.close_menu();
                                                    }
                                                    if ui.add(egui::Button::new("Filter to Virtual Slice View")).clicked() {
                                                        trigger_slice_view = true;
                                                        ui.close_menu();
                                                    }
                                                    ui.separator();
                                                    if ui.add(egui::Button::new("Go to Line...").shortcut_text("Ctrl+G")).clicked() {
                                                        trigger_goto_line = true;
                                                        ui.close_menu();
                                                    }
                                                });

                                                // Folded indicator pill
                                                if let Some((end_line, true)) = folding_targets.get(&line_no) {
                                                    let count = end_line.saturating_sub(line_no);
                                                    let pill = egui::Button::new(
                                                        RichText::new(format!("... [{} lines folded]", count))
                                                            .size(10.0)
                                                            .color(Color32::from_rgb(97, 175, 239))
                                                    ).frame(false);
                                                    if ui.add(pill).on_hover_text("Click to expand").clicked() {
                                                        toggle_fold_for = Some(line_no);
                                                    }
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
                                    });
                                });
                            });
                    }
        }
        self.pending_focus_line = pending_focus;

        // Heatmap Overview Ruler on the right (Option B)
        {
            let matches_guard = self.search_matches.read().unwrap();
            let ruler_props = OverviewRulerProps {
                current_line: self.current_line,
                visible_lines_count: screen_lines.min(total_lines),
                total_lines,
                search_matches: &matches_guard,
                active_match_line,
            };
            let mut ruler_ui = ui.new_child(egui::UiBuilder::new().max_rect(ruler_rect).layout(egui::Layout::top_down(egui::Align::Min)));
            let (target_line, ruler_resp) = render_overview_ruler(&mut ruler_ui, editor_h, &ruler_props);
            if ruler_resp.clicked() || ruler_resp.dragged() || ruler_resp.drag_started() {
                self.mouse_drag_start_line = None;
                self.mouse_drag_start_col = None;
            }
            if let Some(target) = target_line {
                scroll_target = Some(target);
            }
        }

        if let Some(fold_line) = toggle_fold_for {
            if self.folded_lines.contains(&fold_line) {
                self.folded_lines.remove(&fold_line);
            } else {
                self.folded_lines.insert(fold_line);
            }
        }

        if let Some(line) = drag_start_detected {
            self.gutter_drag_active = true;
            self.mouse_drag_start_line = Some(line);
        }

        if clear_gutter_selection {
            self.selection_anchor = None;
            self.selection_head = None;
            self.selection_anchor_col = None;
            self.selection_head_col = None;
        }
        if let Some((a, h)) = new_gutter_selection {
            self.selection_anchor = Some(a);
            self.selection_head = Some(h);
            self.selection_anchor_col = Some(0);
            self.selection_head_col = Some(usize::MAX);
            self.flush_active_line_edit();
            self.active_edit_line = None;
            focused_line = None;
            ui.ctx().memory_mut(|m| m.stop_text_input());
        }
        if let Some((line_no, text, col)) = focused_line {
            self.active_edit_line = Some(line_no);
            self.current_line = line_no;
            self.edit_line_buffer = text.clone();
            self.line_edit_initial_texts.entry(line_no).or_insert(text);
            self.schedule_xpath_resolution(line_no, col, true);
        } else if self.last_resolved_line != self.current_line {
            self.last_resolved_line = self.current_line;
            self.schedule_xpath_resolution(self.current_line, None, false);
        }
        if let Some((line_no, _byte_offset, text)) = line_changed {
            self.edit_line_buffer = text.clone();
            self.xml_validation_result = None;
            self.json_validation_result = None;
            if let Some(ref mut doc) = self.document {
                doc.modified_lines.insert(line_no, text);
            }
            if self.auto_save {
                self.auto_save_timer = Some(Instant::now());
            }
        }
        if let Some((line_no, byte_offset, current_text)) = line_committed {
            if let Some(initial_text) = self.line_edit_initial_texts.remove(&line_no) {
                if initial_text != current_text {
                    let old_len = initial_text.as_bytes().len();
                    let new_len = current_text.as_bytes().len();
                    let delta = (new_len as i64) - (old_len as i64);
                    if let Some(ref mut doc) = self.document {
                        doc.edit_line(line_no, byte_offset, &initial_text, &current_text);
                    }
                    if delta != 0 {
                        for line in &mut self.viewport.lines {
                            if line.line_number > line_no {
                                line.byte_offset = (line.byte_offset as i64 + delta).max(0) as u64;
                            }
                        }
                    }
                }
            }
            if self.active_edit_line == Some(line_no) {
                self.active_edit_line = None;
            }
        }

        if let Some((line_no, col)) = split_requested {
            self.split_line_at_cursor(line_no, col);
        }
        if let Some(line_no) = merge_requested {
            self.merge_line_with_previous(line_no);
        }

        // ArrowUp / ArrowDown navigation between lines when editing
        if ui.input(|i| !i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt && i.key_pressed(Key::ArrowUp)) {
            if let Some(line_no) = self.active_edit_line {
                if line_no > 1 {
                    self.flush_active_line_edit();
                    self.active_edit_line = Some(line_no - 1);
                    if let Some(line) = self.viewport.lines.iter().find(|l| l.line_number == line_no - 1) {
                        self.edit_line_buffer = line.text.clone();
                        self.line_edit_initial_texts.insert(line_no - 1, line.text.clone());
                    }
                    self.current_line = line_no - 1;
                }
            }
        }
        if ui.input(|i| !i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt && i.key_pressed(Key::ArrowDown)) {
            if let Some(line_no) = self.active_edit_line {
                let max_line = self.line_index.as_ref().map(|i| i.total_lines()).unwrap_or(usize::MAX);
                if line_no < max_line {
                    self.flush_active_line_edit();
                    self.active_edit_line = Some(line_no + 1);
                    if let Some(line) = self.viewport.lines.iter().find(|l| l.line_number == line_no + 1) {
                        self.edit_line_buffer = line.text.clone();
                        self.line_edit_initial_texts.insert(line_no + 1, line.text.clone());
                    }
                    self.current_line = line_no + 1;
                }
            }
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

        if trigger_format_modal {
            self.format_options_state.is_open = true;
            self.format_options_state.is_minify = false;
        }
        if trigger_minify {
            self.format_options_state.is_minify = true;
            self.format_options_state.is_open = true;
        }
        if trigger_query_bar {
            self.show_query_bar = !self.show_query_bar;
            if self.show_query_bar && self.query_text.is_empty() {
                if let Some(ref p) = self.current_xpath {
                    self.query_text = p.clone();
                    self.start_query_search(p.clone());
                }
            }
        }
        if trigger_field_extract {
            self.field_extract_state.is_open = true;
        }
        if trigger_slice_view {
            self.activate_virtual_slice_from_query();
        }
        if trigger_goto_line {
            self.show_goto_line_dialog = true;
        }
        if trigger_copy_selection {
            self.copy_selection(ui.ctx());
        }
        if trigger_delete_selection {
            self.delete_selection();
        }
        if trigger_transform_upper {
            self.transform_selection_case(ui.ctx(), true);
        }
        if trigger_transform_lower {
            self.transform_selection_case(ui.ctx(), false);
        }
    }
}

fn compute_line_selection(
    char_sel: Option<((usize, usize), (usize, usize))>,
    line_no: usize,
    line_text: &str,
) -> Option<(usize, usize)> {
    let ((start_line, start_col), (end_line, end_col)) = char_sel?;
    if line_no < start_line || line_no > end_line {
        return None;
    }
    let char_count = line_text.chars().count();
    let char_to_byte = |char_idx: usize| -> usize {
        if char_idx == 0 {
            0
        } else if char_idx >= char_count {
            line_text.len()
        } else {
            line_text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(line_text.len())
        }
    };
    if start_line == end_line {
        if start_col == 0 && end_col == usize::MAX {
            Some((0, line_text.len()))
        } else {
            None
        }
    } else if line_no == start_line {
        let b_start = char_to_byte(start_col);
        if b_start >= line_text.len() {
            None
        } else {
            Some((b_start, line_text.len()))
        }
    } else if line_no == end_line {
        let b_end = char_to_byte(end_col);
        if b_end == 0 {
            None
        } else {
            Some((0, b_end))
        }
    } else {
        Some((0, line_text.len()))
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
    selection: Option<(usize, usize)>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    if wrap_width.is_finite() {
        job.wrap.break_anywhere = true;
    }

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

    let active_bg = if dark_mode {
        Color32::from_rgb(14, 99, 156) // One Dark / VS Code active find blue #0E639C
    } else {
        Color32::from_rgb(166, 213, 255) // Soft sky blue highlight
    };
    let active_fg = if dark_mode {
        Color32::WHITE
    } else {
        Color32::from_rgb(10, 10, 10)
    };
    let inactive_bg = if dark_mode {
        Color32::from_rgba_unmultiplied(65, 95, 140, 150) // One Dark muted slate-blue #415F8C
    } else {
        Color32::from_rgba_unmultiplied(200, 225, 255, 180)
    };
    let inactive_fg = if dark_mode {
        Color32::from_rgb(220, 230, 245)
    } else {
        Color32::from_rgb(20, 20, 20)
    };
    let sel_bg = if dark_mode {
        Color32::from_rgb(61, 69, 86) // One Dark selection slate #3D4556
    } else {
        Color32::from_rgba_unmultiplied(180, 205, 240, 220)
    };

    let mut current_byte_offset = 0;

    for (span_text, span_color) in spans {
        let span_len = span_text.len();
        let span_start = current_byte_offset;
        let span_end = current_byte_offset + span_len;
        current_byte_offset = span_end;

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
                            append_subspan_with_selection(
                                &mut job,
                                &span_text[start_idx..m_start],
                                span_start + start_idx,
                                span_color,
                                &font,
                                selection,
                                sel_bg,
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
                        append_subspan_with_selection(
                            &mut job,
                            &span_text[start_idx..],
                            span_start + start_idx,
                            span_color,
                            &font,
                            selection,
                            sel_bg,
                        );
                    }
                    continue;
                }
            }
        }

        append_subspan_with_selection(
            &mut job,
            &span_text,
            span_start,
            span_color,
            &font,
            selection,
            sel_bg,
        );
    }

    job
}

fn append_subspan_with_selection(
    job: &mut LayoutJob,
    sub_text: &str,
    sub_abs_start: usize,
    color: Color32,
    font: &FontId,
    selection: Option<(usize, usize)>,
    sel_bg: Color32,
) {
    let sub_len = sub_text.len();
    let sub_abs_end = sub_abs_start + sub_len;

    if let Some((sel_start, sel_end)) = selection {
        if sel_start < sel_end && sel_start < sub_abs_end && sel_end > sub_abs_start {
            let overlap_start = sel_start.max(sub_abs_start) - sub_abs_start;
            let overlap_end = sel_end.min(sub_abs_end) - sub_abs_start;

            if sub_text.is_char_boundary(overlap_start) && sub_text.is_char_boundary(overlap_end) {
                if overlap_start > 0 {
                    job.append(
                        &sub_text[..overlap_start],
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color,
                            ..Default::default()
                        },
                    );
                }
                job.append(
                    &sub_text[overlap_start..overlap_end],
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color,
                        background: sel_bg,
                        ..Default::default()
                    },
                );
                if overlap_end < sub_len {
                    job.append(
                        &sub_text[overlap_end..],
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color,
                            ..Default::default()
                        },
                    );
                }
                return;
            }
        }
    }

    job.append(
        sub_text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        },
    );
}

fn replace_in_line(line: &str, query: &SearchQuery, replace_text: &str, replace_all: bool) -> (String, usize) {
    if query.pattern.is_empty() {
        return (line.to_string(), 0);
    }

    if query.is_regex {
        if let Ok(re) = if query.case_sensitive {
            regex::Regex::new(&query.pattern)
        } else {
            regex::RegexBuilder::new(&query.pattern).case_insensitive(true).build()
        } {
            if replace_all {
                let count = re.find_iter(line).count();
                let res = re.replace_all(line, replace_text).into_owned();
                return (res, count);
            } else {
                let found = re.is_match(line);
                let res = re.replace(line, replace_text).into_owned();
                return (res, if found { 1 } else { 0 });
            }
        }
    }

    if query.whole_word {
        let pattern_escaped = regex::escape(&query.pattern);
        let regex_str = format!(r"\b{}\b", pattern_escaped);
        if let Ok(re) = if query.case_sensitive {
            regex::Regex::new(&regex_str)
        } else {
            regex::RegexBuilder::new(&regex_str).case_insensitive(true).build()
        } {
            if replace_all {
                let count = re.find_iter(line).count();
                let res = re.replace_all(line, replace_text).into_owned();
                return (res, count);
            } else {
                let found = re.is_match(line);
                let res = re.replace(line, replace_text).into_owned();
                return (res, if found { 1 } else { 0 });
            }
        }
    }

    if query.case_sensitive {
        if replace_all {
            let count = line.match_indices(&query.pattern).count();
            let res = line.replace(&query.pattern, replace_text);
            return (res, count);
        } else {
            if let Some(pos) = line.find(&query.pattern) {
                let mut res = String::with_capacity(line.len() + replace_text.len());
                res.push_str(&line[..pos]);
                res.push_str(replace_text);
                res.push_str(&line[pos + query.pattern.len()..]);
                return (res, 1);
            }
            return (line.to_string(), 0);
        }
    } else {
        // Case-insensitive literal replacement
        let pattern_escaped = regex::escape(&query.pattern);
        if let Ok(re) = regex::RegexBuilder::new(&pattern_escaped).case_insensitive(true).build() {
            if replace_all {
                let count = re.find_iter(line).count();
                let res = re.replace_all(line, replace_text).into_owned();
                return (res, count);
            } else {
                let found = re.is_match(line);
                let res = re.replace(line, replace_text).into_owned();
                return (res, if found { 1 } else { 0 });
            }
        }
        (line.to_string(), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchQuery;

    #[test]
    fn test_normalize_xml_declaration_line() {
        // Normal valid declaration should return None (no changes needed)
        assert_eq!(
            UltraViewerApp::normalize_xml_declaration_line("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
            None
        );

        // Multiple xml duplicate declaration (user bug) should be repaired
        assert_eq!(
            UltraViewerApp::normalize_xml_declaration_line("<?xml xml xml xml xml xml version='1.0' encoding='UTF-8'?>"),
            Some("<?xml version='1.0' encoding='UTF-8'?>".to_string())
        );

        // Single duplicate xml
        assert_eq!(
            UltraViewerApp::normalize_xml_declaration_line("<?xml xml version='1.0'?>"),
            Some("<?xml version='1.0'?>".to_string())
        );

        // Not an xml declaration
        assert_eq!(
            UltraViewerApp::normalize_xml_declaration_line("<source><job/></source>"),
            None
        );
    }

    #[test]
    fn test_replace_in_line_case_sensitive() {
        let q = SearchQuery {
            pattern: "item".to_string(),
            case_sensitive: true,
            whole_word: false,
            is_regex: false,
        };
        let (res, count) = replace_in_line("<item id='1'><item id='2'></item>", &q, "product", true);
        assert_eq!(count, 3);
        assert_eq!(res, "<product id='1'><product id='2'></product>");

        let (res_one, count_one) = replace_in_line("<item id='1'><item id='2'></item>", &q, "product", false);
        assert_eq!(count_one, 1);
        assert_eq!(res_one, "<product id='1'><item id='2'></item>");
    }

    #[test]
    fn test_replace_in_line_case_insensitive() {
        let q = SearchQuery {
            pattern: "item".to_string(),
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
        };
        let (res, count) = replace_in_line("<Item id='1'><iTeM id='2'></ITEM>", &q, "element", true);
        assert_eq!(count, 3);
        assert_eq!(res, "<element id='1'><element id='2'></element>");
    }

    #[test]
    fn test_replace_in_line_whole_word() {
        let q = SearchQuery {
            pattern: "cat".to_string(),
            case_sensitive: true,
            whole_word: true,
            is_regex: false,
        };
        let (res, count) = replace_in_line("a cat is not a scatter or category cat", &q, "dog", true);
        assert_eq!(count, 2);
        assert_eq!(res, "a dog is not a scatter or category dog");
    }

    #[test]
    fn test_replace_in_line_regex() {
        let q = SearchQuery {
            pattern: r"\b\d{3}\b".to_string(),
            case_sensitive: true,
            whole_word: false,
            is_regex: true,
        };
        let (res, count) = replace_in_line("code 123 and 4567 and 890", &q, "XXX", true);
        assert_eq!(count, 2);
        assert_eq!(res, "code XXX and 4567 and XXX");
    }

    #[test]
    fn test_count_exact_xml_tags_prefix_isolation() {
        // Tag "partner" should not match "<partner_name>" or "<partner_id>"
        let (opens, closes) = UltraViewerApp::count_exact_xml_tags("<partner>", "partner");
        assert_eq!((opens, closes), (1, 0));

        let (opens, closes) = UltraViewerApp::count_exact_xml_tags("<partner_name>John</partner_name>", "partner");
        assert_eq!((opens, closes), (0, 0));

        let (opens, closes) = UltraViewerApp::count_exact_xml_tags("<partner id='1'>", "partner");
        assert_eq!((opens, closes), (1, 0));

        let (opens, closes) = UltraViewerApp::count_exact_xml_tags("<partner id='1'/>", "partner");
        assert_eq!((opens, closes), (0, 0));

        let (opens, closes) = UltraViewerApp::count_exact_xml_tags("</partner>", "partner");
        assert_eq!((opens, closes), (0, 1));
    }

    #[test]
    fn test_find_folding_end_nested_xml() {
        use crate::editor::ViewportLine;

        let lines = vec![
            ViewportLine { line_number: 1, byte_offset: 0, text: "<root>".to_string(), is_truncated: false },
            ViewportLine { line_number: 2, byte_offset: 7, text: "  <partner>".to_string(), is_truncated: false },
            ViewportLine { line_number: 3, byte_offset: 19, text: "    <partner_name>ACME</partner_name>".to_string(), is_truncated: false },
            ViewportLine { line_number: 4, byte_offset: 57, text: "    <partner_type>Corp</partner_type>".to_string(), is_truncated: false },
            ViewportLine { line_number: 5, byte_offset: 95, text: "  </partner>".to_string(), is_truncated: false },
            ViewportLine { line_number: 6, byte_offset: 108, text: "</root>".to_string(), is_truncated: false },
        ];

        // Line 2 (<partner>) should fold exactly to line 5 (</partner>), NOT line 6
        let partner_fold = UltraViewerApp::find_folding_end(&lines, 1);
        assert_eq!(partner_fold, Some(5));

        // Line 1 (<root>) should fold to line 6 (</root>)
        let root_fold = UltraViewerApp::find_folding_end(&lines, 0);
        assert_eq!(root_fold, Some(6));

        // Line 3 has no folding since it closes on the same line
        let leaf_fold = UltraViewerApp::find_folding_end(&lines, 2);
        assert_eq!(leaf_fold, None);
    }

    #[test]
    fn test_extract_sample_xml_with_record_tag() {
        let xml = r#"<source>
  <publisher>Acme Corp</publisher>
  <job id="101">
    <title>Software Engineer</title>
    <location>Remote</location>
  </job>
  <job id="102">
    <title>DevOps Specialist</title>
  </job>
</source>"#;

        let res = UltraViewerApp::extract_sample_xml(xml.as_bytes(), Some("job"));
        assert!(res.is_some());
        let (tag, bytes) = res.unwrap();
        assert_eq!(tag, "job");
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("<job id=\"101\">"));
        assert!(text.contains("<title>Software Engineer</title>"));
        assert!(text.ends_with("</job>"));
    }

    #[test]
    fn test_extract_sample_json() {
        // Test array of objects
        let json_arr = r#"[
  {
    "id": 1,
    "title": "Engineer"
  },
  {
    "id": 2,
    "title": "Designer"
  }
]"#;
        let res = UltraViewerApp::extract_sample_json(json_arr.as_bytes());
        assert!(res.is_some());
        let snippet = String::from_utf8(res.unwrap()).unwrap();
        assert!(snippet.starts_with('{'));
        assert!(snippet.ends_with('}'));
        assert!(snippet.contains("\"id\": 1"));
        assert!(!snippet.contains("\"id\": 2"));

        // Test root object containing array
        let json_nested = r#"{
  "status": "ok",
  "records": [
    {
      "code": 404,
      "msg": "Not Found"
    }
  ]
}"#;
        let res_nested = UltraViewerApp::extract_sample_json(json_nested.as_bytes());
        assert!(res_nested.is_some());
        let snippet_nested = String::from_utf8(res_nested.unwrap()).unwrap();
        assert!(snippet_nested.contains("\"code\": 404"));
        assert!(!snippet_nested.contains("\"status\": \"ok\""));
    }

    #[test]
    fn test_split_and_merge_lines() {
        let mut app = UltraViewerApp::default();
        let content = b"first line\nsecond line\nthird line\n";
        let temp_dir = std::env::temp_dir();
        let p = temp_dir.join("test_split_merge.txt");
        std::fs::write(&p, content).unwrap();

        app.open_file(&p);
        assert_eq!(app.viewport.lines.len(), 3);
        assert_eq!(app.viewport.lines[1].text, "second line");

        // Split "second line" at index 6 ("second" and " line")
        app.split_line_at_cursor(2, 6);
        assert_eq!(app.viewport.lines.len(), 4);
        assert_eq!(app.viewport.lines[1].line_number, 2);
        assert_eq!(app.viewport.lines[1].text, "second");
        assert_eq!(app.viewport.lines[2].line_number, 3);
        assert_eq!(app.viewport.lines[2].text, " line");
        assert_eq!(app.viewport.lines[3].line_number, 4);
        assert_eq!(app.viewport.lines[3].text, "third line");

        // Merge line 3 back into line 2
        app.merge_line_with_previous(3);
        assert_eq!(app.viewport.lines.len(), 3);
        assert_eq!(app.viewport.lines[1].line_number, 2);
        assert_eq!(app.viewport.lines[1].text, "second line");
        assert_eq!(app.viewport.lines[2].line_number, 3);
        assert_eq!(app.viewport.lines[2].text, "third line");

        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_delete_selection_multiline() {
        let mut app = UltraViewerApp::default();
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5\n";
        let temp_dir = std::env::temp_dir();
        let p = temp_dir.join("test_delete_selection.txt");
        std::fs::write(&p, content).unwrap();

        app.open_file(&p);
        assert_eq!(app.viewport.lines.len(), 5);

        // Select lines 2 to 4
        app.selection_anchor = Some(2);
        app.selection_head = Some(4);
        assert_eq!(app.selection_range(), Some((2, 4)));

        app.delete_selection();
        assert_eq!(app.viewport.lines.len(), 2);
        assert_eq!(app.viewport.lines[0].line_number, 1);
        assert_eq!(app.viewport.lines[0].text, "line 1");
        assert_eq!(app.viewport.lines[1].line_number, 2);
        assert_eq!(app.viewport.lines[1].text, "line 5");

        // Simulate scrolling away and scrolling back up while document is unsaved
        app.viewport.lines.clear();
        app.scroll_to_line(1);
        assert_eq!(app.viewport.lines.len(), 2);
        assert_eq!(app.viewport.lines[0].text, "line 1");
        assert_eq!(app.viewport.lines[1].text, "line 5");

        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_xml_validation_recovers_after_piece_table_edit() {
        let mut app = UltraViewerApp::default();
        let invalid_xml = b"<root>\n  <item>text\n</root>\n";
        let temp_dir = std::env::temp_dir();
        let p = temp_dir.join("test_xml_recovery.xml");
        std::fs::write(&p, invalid_xml).unwrap();

        app.open_file(&p);
        app.validate_xml_document();
        if let Some(rx) = app.xml_validation_rx.take() {
            app.xml_validation_result = Some(rx.recv().unwrap());
            app.is_validating_xml = false;
        }
        assert!(matches!(app.xml_validation_result, Some(crate::formats::xml::XmlValidationResult::Invalid { .. })));

        // Fix the XML by editing line 2 from "  <item>text" to "  <item>text</item>"
        if let Some(ref mut doc) = app.document {
            doc.edit_line(2, 7, "  <item>text", "  <item>text</item>");
        }

        app.validate_xml_document();
        if let Some(rx) = app.xml_validation_rx.take() {
            app.xml_validation_result = Some(rx.recv().unwrap());
            app.is_validating_xml = false;
        }
        assert!(matches!(app.xml_validation_result, Some(crate::formats::xml::XmlValidationResult::Valid { .. })));

        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_delete_blank_line_with_backspace() {
        let mut app = UltraViewerApp::default();
        let content = b"<partner_attributes>\n\n\n<title>Test</title>\n";
        let temp_dir = std::env::temp_dir();
        let p = temp_dir.join("test_blank_lines.xml");
        std::fs::write(&p, content).unwrap();

        app.open_file(&p);
        assert_eq!(app.viewport.lines.len(), 4);
        assert_eq!(app.viewport.lines[1].text, "");
        assert_eq!(app.viewport.lines[2].text, "");
        assert_eq!(app.viewport.lines[3].text, "<title>Test</title>");

        // Backspace on line 3 (empty line) merges with line 2
        app.merge_line_with_previous(3);
        assert_eq!(app.viewport.lines.len(), 3);
        assert_eq!(app.viewport.lines[0].line_number, 1);
        assert_eq!(app.viewport.lines[1].line_number, 2);
        assert_eq!(app.viewport.lines[1].text, "");
        assert_eq!(app.viewport.lines[2].line_number, 3);
        assert_eq!(app.viewport.lines[2].text, "<title>Test</title>");
        assert_eq!(app.pending_focus_line, Some((2, 0)));

        // Backspace on line 2 (empty line) merges with line 1
        app.merge_line_with_previous(2);
        assert_eq!(app.viewport.lines.len(), 2);
        assert_eq!(app.viewport.lines[0].line_number, 1);
        assert_eq!(app.viewport.lines[0].text, "<partner_attributes>");
        assert_eq!(app.viewport.lines[1].line_number, 2);
        assert_eq!(app.viewport.lines[1].text, "<title>Test</title>");
        assert_eq!(app.pending_focus_line, Some((1, 20)));

        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_word_wrap_unbroken_tokens() {
        let url = "<url><![CDATA[https://kimco.thejobnetwork.com/Job/604233562?etd=MUVP6CXQ22IN5GBF6Z5ZZ4KP573GWG4NSVRU5OLDR6VHFMHKODJLVZG3SZYF4EQSZRDCHFHYXIZ67EKC575NYWPE2TCVVBX2X2OG2SUBMBXLCU7GPZTKYE67QM6KM5266BSQOMSUBD3JW%3d%3d%3d]]></url>";
        let font = egui::FontId::monospace(14.0);
        let job = build_line_layout_job(
            url,
            Some(crate::FileType::Xml),
            true,
            None,
            false,
            false,
            font,
            true,
            400.0,
            None,
        );
        assert_eq!(job.wrap.max_width, 400.0);
        assert!(job.wrap.break_anywhere);
    }

    #[test]
    fn test_no_wrap_layout_uses_unbounded_width() {
        let job = build_line_layout_job(
            "<description>long unbroken content</description>",
            Some(crate::FileType::Xml),
            true,
            None,
            false,
            false,
            egui::FontId::monospace(14.0),
            true,
            f32::INFINITY,
            None,
        );
        assert!(job.wrap.max_width.is_infinite());
        assert!(!job.wrap.break_anywhere);
    }

    #[test]
    fn test_2d_multiline_selection_range() {
        let mut app = UltraViewerApp::default();
        let line1 = "<company>Acme Corp</company>";
        let line2 = "<city>New York</city>";
        let line3 = "<country>USA</country>";

        // Dragging downwards: Line 1 col 9 ("Acme...") to Line 3 col 9 ("<country>")
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(9);
        app.selection_head = Some(3);
        app.selection_head_col = Some(9);

        let sel_range = app.char_selection_range();
        assert_eq!(sel_range, Some(((1, 9), (3, 9))));

        // Line 1: from byte 9 to end
        assert_eq!(compute_line_selection(sel_range, 1, line1), Some((9, line1.len())));
        // Line 2: entire line (middle)
        assert_eq!(compute_line_selection(sel_range, 2, line2), Some((0, line2.len())));
        // Line 3: from 0 to byte 9
        assert_eq!(compute_line_selection(sel_range, 3, line3), Some((0, 9)));

        // Dragging upwards: Line 3 col 9 to Line 1 col 9
        app.selection_anchor = Some(3);
        app.selection_anchor_col = Some(9);
        app.selection_head = Some(1);
        app.selection_head_col = Some(9);

        let sel_range_up = app.char_selection_range();
        assert_eq!(sel_range_up, Some(((1, 9), (3, 9))));
        assert_eq!(compute_line_selection(sel_range_up, 1, line1), Some((9, line1.len())));
        assert_eq!(compute_line_selection(sel_range_up, 2, line2), Some((0, line2.len())));
        assert_eq!(compute_line_selection(sel_range_up, 3, line3), Some((0, 9)));

        // Single line drag/click returns None (handled natively by TextEdit)
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(3);
        app.selection_head = Some(1);
        app.selection_head_col = Some(10);
        let sel_single = app.char_selection_range();
        assert_eq!(compute_line_selection(sel_single, 1, line1), None);

        // Gutter single line selection returns entire line
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(0);
        app.selection_head = Some(1);
        app.selection_head_col = Some(usize::MAX);
        let sel_gutter = app.char_selection_range();
        assert_eq!(compute_line_selection(sel_gutter, 1, line1), Some((0, line1.len())));
    }

    #[test]
    fn test_transform_selection_case_inline_word() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        let active_line = 37;
        let initial = "  <category><![CDATA[Building Maintenance]]></category>".to_string();
        app.active_edit_line = Some(active_line);
        app.edit_line_buffer = initial.clone();
        app.viewport.lines.push(ViewportLine {
            line_number: active_line,
            byte_offset: 100,
            text: initial.clone(),
            is_truncated: false,
        });

        // Select only "Maintenance" (chars 30..41)
        let edit_id = egui::Id::new("line_editor").with(active_line);
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(30),
            egui::text::CCursor::new(41),
        )));
        state.store(&ctx, edit_id);

        // Transform to uppercase
        app.transform_selection_case(&ctx, true);

        let expected_upper = "  <category><![CDATA[Building MAINTENANCE]]></category>";
        assert_eq!(app.edit_line_buffer, expected_upper);
        assert_eq!(app.viewport.lines[0].text, expected_upper);
        assert_eq!(app.document.as_ref().unwrap().modified_lines.get(&active_line).unwrap(), expected_upper);

        // Transform to lowercase
        app.transform_selection_case(&ctx, false);
        let expected_lower = "  <category><![CDATA[Building maintenance]]></category>";
        assert_eq!(app.edit_line_buffer, expected_lower);
        assert_eq!(app.viewport.lines[0].text, expected_lower);
    }

    #[test]
    fn test_transform_selection_case_single_line_drag() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        app.viewport.lines.push(ViewportLine {
            line_number: 1,
            byte_offset: 0,
            text: "<title>hello</title>".to_string(),
            is_truncated: false,
        });

        // Selection on line 1 from col 7 to col 12 (only "hello")
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(7);
        app.selection_head = Some(1);
        app.selection_head_col = Some(12);

        app.transform_selection_case(&ctx, true);

        assert_eq!(app.viewport.lines[0].text, "<title>HELLO</title>");
    }

    #[test]
    fn test_transform_selection_case_multiline() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        app.viewport.lines.push(ViewportLine {
            line_number: 1,
            byte_offset: 0,
            text: "<title>hello</title>".to_string(),
            is_truncated: false,
        });
        app.viewport.lines.push(ViewportLine {
            line_number: 2,
            byte_offset: 21,
            text: "<desc>world</desc>".to_string(),
            is_truncated: false,
        });

        // Selection from line 1 col 7 ('h' of hello) to line 2 col 11 (after 'world')
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(7);
        app.selection_head = Some(2);
        app.selection_head_col = Some(11);

        app.transform_selection_case(&ctx, true);

        // Line 1 transformed from col 7 to end
        assert_eq!(app.viewport.lines[0].text, "<title>HELLO</TITLE>");
        // Line 2 transformed from start to col 11
        assert_eq!(app.viewport.lines[1].text, "<DESC>WORLD</desc>");
    }

    #[test]
    fn test_transform_selection_case_whole_line_fallback() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        app.current_line = 5;
        app.viewport.lines.push(ViewportLine {
            line_number: 5,
            byte_offset: 50,
            text: "<item>apple</item>".to_string(),
            is_truncated: false,
        });

        app.transform_selection_case(&ctx, true);

        assert_eq!(app.viewport.lines[0].text, "<ITEM>APPLE</ITEM>");

        // Also test whole-line fallback for lowercase
        app.transform_selection_case(&ctx, false);
        assert_eq!(app.viewport.lines[0].text, "<item>apple</item>");
    }

    #[test]
    fn test_transform_selection_case_inline_lowercase() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        let active_line = 12;
        let initial = "<job><title>SENIOR SOFTWARE ENGINEER</title></job>".to_string();
        app.active_edit_line = Some(active_line);
        app.edit_line_buffer = initial.clone();
        app.viewport.lines.push(ViewportLine {
            line_number: active_line,
            byte_offset: 200,
            text: initial.clone(),
            is_truncated: false,
        });

        // Select only "SOFTWARE" (chars 19..27)
        let edit_id = egui::Id::new("line_editor").with(active_line);
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(19),
            egui::text::CCursor::new(27),
        )));
        state.store(&ctx, edit_id);

        // Transform to lowercase
        app.transform_selection_case(&ctx, false);

        let expected = "<job><title>SENIOR software ENGINEER</title></job>";
        assert_eq!(app.edit_line_buffer, expected);
        assert_eq!(app.viewport.lines[0].text, expected);
        assert_eq!(app.document.as_ref().unwrap().modified_lines.get(&active_line).unwrap(), expected);
    }

    #[test]
    fn test_transform_selection_case_multiline_lowercase() {
        let ctx = egui::Context::default();
        let mut app = UltraViewerApp::default();
        app.viewport.lines.push(ViewportLine {
            line_number: 1,
            byte_offset: 0,
            text: "<TAG>HELLO WORLD</TAG>".to_string(),
            is_truncated: false,
        });
        app.viewport.lines.push(ViewportLine {
            line_number: 2,
            byte_offset: 25,
            text: "<DATA>FOO BAR</DATA>".to_string(),
            is_truncated: false,
        });

        // Selection from line 1 col 5 ("HELLO WORLD</TAG>") to line 2 col 14 ("<DATA>FOO BAR")
        app.selection_anchor = Some(1);
        app.selection_anchor_col = Some(5);
        app.selection_head = Some(2);
        app.selection_head_col = Some(14);

        app.transform_selection_case(&ctx, false);

        assert_eq!(app.viewport.lines[0].text, "<TAG>hello world</tag>");
        assert_eq!(app.viewport.lines[1].text, "<data>foo bar</DATA>");
    }

    #[test]
    fn test_scroll_wheel_smooth_accumulator() {
        let mut app = UltraViewerApp::default();
        app.current_line = 50;

        // Test standard Windows mouse wheel notch (120 units) -> 5 lines per notch
        let notch_delta: f32 = -120.0; // scroll down 1 notch
        let points_per_line = 24.0_f32; // 120 / 24 = 5 lines per notch
        app.scroll_accumulator += notch_delta;
        let lines = (app.scroll_accumulator / points_per_line).trunc() as isize;
        assert_eq!(lines, -5);
        app.scroll_accumulator -= lines as f32 * points_per_line;
        assert_eq!(app.scroll_accumulator, 0.0);

        // Test smooth trackpad partial gestures (e.g. 5 units per frame)
        app.scroll_accumulator += -5.0;
        let lines_partial = (app.scroll_accumulator / 12.0).trunc() as isize;
        assert_eq!(lines_partial, 0); // No premature jump

        app.scroll_accumulator += -10.0; // total -15.0
        let lines_smooth = (app.scroll_accumulator / 12.0).trunc() as isize;
        assert_eq!(lines_smooth, -1); // Exactly 1 smooth line
        app.scroll_accumulator -= lines_smooth as f32 * 12.0;
        assert_eq!(app.scroll_accumulator, -3.0); // clean remainder preserved
    }
}

