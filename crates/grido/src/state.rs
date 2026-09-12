//! Application state shared by all UI modules.
//!
//! `GridoApp` lives here; `app.rs` owns the update loop and command dispatch,
//! and each `ui::*` module adds its own `impl GridoApp` block.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::engine::{Engine, FindHit, Range};
use crate::keymap::Keymap;

pub const GUTTER_W: f32 = 48.0;
pub const HEADER_H: f32 = 24.0;
pub const BASE_FONT: f32 = 13.0;
pub const CELL_PAD: f32 = 4.0;
/// Width of the hit zone for dragging a header edge to resize.
pub const RESIZE_GRAB: f32 = 4.0;

/// Spreadsheets distinguish these two modes: after typing into a cell, arrow keys
/// commit and move; after F2 (or double-click), they move the caret instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditMode {
    /// Started by typing — arrows commit the edit and move the cursor.
    Enter,
    /// Started by F2 / double-click / formula bar — arrows move the caret.
    Edit,
}

pub struct EditState {
    pub row: i32,
    pub col: i32,
    pub text: String,
    pub mode: EditMode,
    pub take_focus: bool,
    pub caret_end: bool,
    pub in_formula_bar: bool,
    /// Function-name suggestions for the current text, and which is highlighted.
    pub completions: Vec<String>,
    pub completion_index: usize,
    /// Text the completion list was computed for, so we only recompute on change.
    pub completions_for: String,
}

impl EditState {
    pub fn new(row: i32, col: i32, text: String, mode: EditMode, in_formula_bar: bool) -> EditState {
        EditState {
            row,
            col,
            text,
            mode,
            take_focus: !in_formula_bar,
            caret_end: true,
            in_formula_bar,
            completions: Vec::new(),
            completion_index: 0,
            completions_for: String::new(),
        }
    }

    pub fn is_formula(&self) -> bool {
        self.text.starts_with('=')
    }
}

#[derive(Clone, PartialEq)]
pub enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Home,
    Insert,
    Data,
    View,
    Sheet,
    Help,
}

/// Which modal dialog, if any, is open.
#[derive(Clone, PartialEq)]
pub enum Dialog {
    None,
    FindReplace,
    Sort,
    NameManager,
    FormatCells,
    Shortcuts,
    RenameSheet { sheet: u32, name: String },
    GotoCell,
    ConditionalFormat,
    Chart,
}

/// Draft state for the conditional-formatting dialog.
pub struct CfState {
    /// 0 = cell value rule, 1 = color scale
    pub kind: usize,
    /// 0 greater, 1 less, 2 equal, 3 between
    pub operator: usize,
    pub value: String,
    pub value2: String,
    pub fill: [u8; 3],
    pub message: String,
}

impl Default for CfState {
    fn default() -> Self {
        CfState {
            kind: 0,
            operator: 0,
            value: "0".to_string(),
            value2: String::new(),
            fill: [255, 199, 206],
            message: String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
}

/// A chart built from a range and drawn natively (not stored in the xlsx).
pub struct Chart {
    pub kind: ChartKind,
    pub sheet: u32,
    pub data: Range,
    pub title: String,
    /// First row of the range holds series labels.
    pub has_header: bool,
}

/// A drag in progress on a row/column header edge.
#[derive(Clone, Copy, PartialEq)]
pub enum Resizing {
    Column { index: i32, start_x: f32, start_w: f32 },
    Row { index: i32, start_y: f32, start_h: f32 },
}

/// State of the fill handle drag (the little square at the selection corner).
#[derive(Clone, Copy, PartialEq)]
pub struct FillDrag {
    pub origin: Range,
    pub to_row: i32,
    pub to_col: i32,
}

/// An active AutoFilter on a header row.
#[derive(Clone)]
pub struct Filter {
    pub header_row: i32,
    pub r0: i32,
    pub r1: i32,
    pub c0: i32,
    pub c1: i32,
    /// column index -> set of allowed display values (empty = all allowed)
    pub allowed: HashMap<i32, Vec<String>>,
    /// Which column's dropdown is currently open.
    pub open_column: Option<i32>,
}

pub struct FindState {
    pub needle: String,
    pub replacement: String,
    pub match_case: bool,
    pub whole_workbook: bool,
    pub hits: Vec<FindHit>,
    pub index: usize,
    pub message: String,
}

impl Default for FindState {
    fn default() -> Self {
        FindState {
            needle: String::new(),
            replacement: String::new(),
            match_case: false,
            whole_workbook: false,
            hits: Vec::new(),
            index: 0,
            message: String::new(),
        }
    }
}

pub struct SortState {
    pub key_column: i32,
    pub ascending: bool,
    pub has_header: bool,
}

impl Default for SortState {
    fn default() -> Self {
        SortState {
            key_column: 1,
            ascending: true,
            has_header: true,
        }
    }
}

pub struct NameState {
    pub new_name: String,
    pub new_formula: String,
    pub message: String,
}

impl Default for NameState {
    fn default() -> Self {
        NameState {
            new_name: String::new(),
            new_formula: String::new(),
            message: String::new(),
        }
    }
}

/// A destructive thing the assistant asked for, held until the user approves.
#[derive(Clone, Debug, PartialEq)]
pub enum AssistantAction {
    DeleteRows { sheet: u32, at: i32, count: i32 },
}

/// A pending confirmation raised by an assistant tool call.
#[derive(Clone, Debug)]
pub struct AssistantRequest {
    pub prompt: String,
    pub action: AssistantAction,
}

/// A range the assistant recently wrote, tinted in the grid until it fades.
#[derive(Clone, Copy, Debug)]
pub struct RecentChange {
    pub sheet: u32,
    pub range: Range,
    /// Seconds of fade remaining.
    pub ttl: f32,
}

/// One line in the assistant activity log.
#[derive(Clone, Debug)]
pub struct Activity {
    pub text: String,
    pub ok: bool,
}

pub struct GridoApp {
    pub engine: Engine,
    pub keymap: Keymap,
    pub sheet: u32,

    // Selection: cursor is the active cell, anchor the other corner.
    pub cursor: (i32, i32),
    pub anchor: (i32, i32),
    pub saved_cursors: HashMap<u32, ((i32, i32), (i32, i32))>,

    pub edit: Option<EditState>,
    pub scroll_to_cursor: bool,

    // Grid geometry cache: boundary offsets in px (already zoom-scaled).
    // row_off[r] is the y where row r+1 starts; row r (1-based) spans
    // row_off[r-1]..row_off[r].
    pub row_off: Vec<f32>,
    pub col_off: Vec<f32>,
    pub view_rows: i32,
    pub view_cols: i32,
    pub geom_dirty: bool,
    pub rows_per_page: i32,
    pub zoom: f32,

    // Interactions in progress.
    pub resizing: Option<Resizing>,
    pub fill_drag: Option<FillDrag>,
    /// Cells being cut, shown with a dashed outline.
    pub cut_marker: Option<(u32, Range)>,

    // Dialog / modal state.
    pub dialog: Dialog,
    pub find: FindState,
    pub sort: SortState,
    pub names: NameState,
    pub goto_text: String,
    pub filter: Option<Filter>,
    pub cf: CfState,
    /// Charts drawn in floating windows over the grid.
    pub charts: Vec<Chart>,
    pub chart_draft: ChartKind,

    // File / lifecycle.
    pub pending: Option<Pending>,
    pub allow_close: bool,
    pub status: String,
    pub last_title: String,
    pub ribbon_tab: RibbonTab,
    pub backstage: bool,
    pub recent: Vec<PathBuf>,
    pub recent_colors: Vec<[u8; 3]>,

    // Assistant (MCP) session state.
    /// Queue of tool calls waiting to run on this thread.
    pub mcp_rx: Option<std::sync::mpsc::Receiver<crate::mcp::Call>>,
    /// True when this instance owns the MCP socket.
    pub mcp_serving: bool,
    /// The socket the host serves MCP on, so the File view can tell whether
    /// another window is the one an assistant is talking to.
    pub mcp_socket: Option<PathBuf>,
    /// How many engine undo entries each logical action produced, newest last.
    pub undo_groups: Vec<usize>,
    /// Ranges the assistant just wrote, tinted until they fade.
    pub recent_changes: Vec<RecentChange>,
    /// What the assistant has done this session, newest first.
    pub activity: Vec<Activity>,
    /// Shown as a modal when a tool wants to do something destructive.
    pub assistant_request: Option<AssistantRequest>,
    /// Set once the assistant has written to the current file, so the backup
    /// is taken once per session rather than on every edit.
    pub backed_up: bool,
    pub show_activity: bool,
    /// Follows the Omarchy theme and restyles live when it changes.
    pub theme_watcher: enclave_ui::theme::ThemeWatcher,
    /// Seconds of quiet remaining before the assistant's work is autosaved.
    pub autosave_in: Option<f32>,
}

impl GridoApp {
    /// Records that the assistant touched a range, for the fading highlight.
    pub fn note_change(&mut self, sheet: u32, range: Range) {
        self.recent_changes.push(RecentChange {
            sheet,
            range,
            ttl: 6.0,
        });
        if self.recent_changes.len() > 64 {
            self.recent_changes.remove(0);
        }
    }

    /// Remembers that one logical action produced `steps` undo entries, so a
    /// single Ctrl+Z can undo the whole thing.
    pub fn push_undo_group(&mut self, steps: usize) {
        if steps > 1 {
            self.undo_groups.push(steps);
        }
    }

    /// Undo one logical action: a whole assistant edit if the last thing done
    /// was one, otherwise a single step.
    pub fn undo_grouped(&mut self) {
        let steps = self.undo_groups.pop().unwrap_or(1);
        for _ in 0..steps {
            self.engine.undo();
        }
        self.invalidate_geometry();
    }

    /// Queues a destructive request for the user to approve.
    pub fn request_confirmation(&mut self, prompt: String, action: AssistantAction) {
        self.assistant_request = Some(AssistantRequest { prompt, action });
    }

    pub fn log_activity(&mut self, text: impl Into<String>, ok: bool) {
        self.activity.insert(
            0,
            Activity {
                text: text.into(),
                ok,
            },
        );
        self.activity.truncate(200);
    }

    /// Replaces the workbook with an empty one (used by templates).
    pub fn new_workbook(&mut self) {
        if let Ok(engine) = Engine::new() {
            self.engine = engine;
            self.sheet = 0;
            self.cursor = (1, 1);
            self.anchor = (1, 1);
            self.saved_cursors.clear();
            self.edit = None;
            self.undo_groups.clear();
            self.recent_changes.clear();
            self.backed_up = false;
            self.reset_view();
        }
    }

    /// The selected block, normalized so r0 <= r1 and c0 <= c1.
    pub fn selection(&self) -> Range {
        Range {
            r0: self.cursor.0.min(self.anchor.0),
            r1: self.cursor.0.max(self.anchor.0),
            c0: self.cursor.1.min(self.anchor.1),
            c1: self.cursor.1.max(self.anchor.1),
        }
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    /// Reports an error from a fallible engine call in the status bar.
    pub fn report(&mut self, context: &str, result: anyhow::Result<()>) {
        if let Err(e) = result {
            self.status = format!("{context}: {e}");
        }
    }

    pub fn invalidate_geometry(&mut self) {
        self.geom_dirty = true;
    }
}
