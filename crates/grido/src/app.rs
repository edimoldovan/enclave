//! Update loop, input handling and command dispatch.
//!
//! Rendering lives in the `ui` modules, each of which adds methods to
//! `GridoApp` (defined in `state.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{Context, Event, ViewportCommand};

use crate::commands::Command;
use crate::engine::{
    BorderTarget, Engine, HAlign, Range, StyleEdit, MAX_COLS, MAX_ROWS,
};
use crate::state::{
    Dialog, EditMode, EditState, FindState, NameState, Pending, RibbonTab, GridoApp, SortState,
};

const DEFAULT_FILL_COLORS: [[u8; 3]; 8] = [
    [255, 255, 255],
    [255, 242, 204],
    [226, 239, 218],
    [222, 235, 247],
    [252, 228, 214],
    [237, 237, 237],
    [255, 217, 217],
    [230, 224, 236],
];

fn recent_file() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/grido/recent.txt"))
}

/// Moves a pre-rename `~/.config/sheetz/recent.txt` into place, once. After
/// this the old path is never read again.
fn migrate_recent(path: &Path) {
    if path.exists() {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let old = PathBuf::from(home).join(".config/sheetz/recent.txt");
    if !old.is_file() {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::rename(&old, path);
}

fn load_recent() -> Vec<PathBuf> {
    let Some(path) = recent_file() else {
        return Vec::new();
    };
    migrate_recent(&path);
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .take(10)
        .collect()
}

impl GridoApp {
    pub fn new(path: Option<PathBuf>) -> GridoApp {
        let (keymap, warnings) = crate::keymap::load();
        let mut status = warnings.join(" | ");
        let engine = match &path {
            Some(p) => Engine::open(p).unwrap_or_else(|e| {
                status = format!("Could not open {}: {e}", p.display());
                Engine::new().expect("empty workbook")
            }),
            None => Engine::new().expect("empty workbook"),
        };
        let mut app = GridoApp {
            engine,
            keymap,
            sheet: 0,
            cursor: (1, 1),
            anchor: (1, 1),
            saved_cursors: HashMap::new(),
            edit: None,
            scroll_to_cursor: false,
            row_off: Vec::new(),
            col_off: Vec::new(),
            view_rows: 0,
            view_cols: 0,
            geom_dirty: true,
            rows_per_page: 30,
            zoom: 1.0,
            resizing: None,
            fill_drag: None,
            cut_marker: None,
            dialog: Dialog::None,
            find: FindState::default(),
            sort: SortState::default(),
            names: NameState::default(),
            goto_text: String::new(),
            filter: None,
            cf: crate::state::CfState::default(),
            charts: Vec::new(),
            chart_draft: crate::state::ChartKind::Bar,
            pending: None,
            allow_close: false,
            status,
            last_title: String::new(),
            ribbon_tab: RibbonTab::Home,
            backstage: false,
            recent: load_recent(),
            recent_colors: DEFAULT_FILL_COLORS.to_vec(),
            mcp_rx: None,
            mcp_serving: false,
            mcp_socket: None,
            undo_groups: Vec::new(),
            recent_changes: Vec::new(),
            activity: Vec::new(),
            assistant_request: None,
            backed_up: false,
            show_activity: false,
            theme_watcher: enclave_ui::theme::ThemeWatcher::new(),
            autosave_in: None,
        };
        if let Some(p) = &path {
            if app.engine.path.is_some() {
                app.push_recent(&p.clone());
            }
        }
        app.reset_view();
        app
    }

    // ----- view bookkeeping -----

    pub fn reset_view(&mut self) {
        let (ur, uc) = self.engine.used_range(self.sheet);
        self.view_rows = (ur + 50).clamp(200, MAX_ROWS);
        self.view_cols = (uc + 10).clamp(30, MAX_COLS);
        self.geom_dirty = true;
    }

    pub fn ensure_view_contains(&mut self, row: i32, col: i32) {
        if row > self.view_rows - 2 {
            self.view_rows = (row + 100).min(MAX_ROWS);
            self.geom_dirty = true;
        }
        if col > self.view_cols - 2 {
            self.view_cols = (col + 10).min(MAX_COLS);
            self.geom_dirty = true;
        }
    }

    // ----- cursor movement -----

    pub fn move_cursor(&mut self, dr: i32, dc: i32, extend: bool) {
        self.cursor.0 = (self.cursor.0 + dr).clamp(1, MAX_ROWS);
        self.cursor.1 = (self.cursor.1 + dc).clamp(1, MAX_COLS);
        if !extend {
            self.anchor = self.cursor;
        }
        self.ensure_view_contains(self.cursor.0, self.cursor.1);
        self.scroll_to_cursor = true;
    }

    pub fn set_cursor(&mut self, row: i32, col: i32) {
        self.cursor = (row.clamp(1, MAX_ROWS), col.clamp(1, MAX_COLS));
        self.anchor = self.cursor;
        self.ensure_view_contains(self.cursor.0, self.cursor.1);
        self.scroll_to_cursor = true;
    }

    /// Ctrl+Arrow: jump to the boundary of the current data region.
    pub fn jump(&mut self, dr: i32, dc: i32, extend: bool) {
        let (ur, uc) = self.engine.used_range(self.sheet);
        let (r, c) = self.cursor;
        let sheet = self.sheet;
        let filled = |r: i32, c: i32| !self.engine.is_empty(sheet, r, c);
        let (target_r, target_c);
        if dr != 0 {
            let far = if dr > 0 { MAX_ROWS } else { 1 };
            let scan_end = if dr > 0 { ur } else { 1 };
            target_r = jump_axis(r, dr, far, scan_end, |i| filled(i, c));
            target_c = c;
        } else {
            let far = if dc > 0 { MAX_COLS } else { 1 };
            let scan_end = if dc > 0 { uc } else { 1 };
            target_c = jump_axis(c, dc, far, scan_end, |i| filled(r, i));
            target_r = r;
        }
        if extend {
            self.cursor = (target_r, target_c);
            self.ensure_view_contains(target_r, target_c);
            self.scroll_to_cursor = true;
        } else {
            self.set_cursor(target_r, target_c);
        }
    }

    // ----- clipboard -----

    fn copy_selection(&mut self, ctx: &Context, cut: bool) {
        let sel = self.selection();
        let sheet = self.sheet;
        match self.engine.copy(sheet, sel) {
            Ok(tsv) => {
                ctx.copy_text(tsv);
                self.cut_marker = if cut { Some((sheet, sel)) } else { None };
            }
            Err(e) => self.set_status(format!("Copy: {e}")),
        }
    }

    fn paste(&mut self, values_only: bool) {
        let text = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .unwrap_or_default();
        let at = (self.cursor.0.min(self.anchor.0), self.cursor.1.min(self.anchor.1));
        let sheet = self.sheet;

        // Prefer the internal (formula-aware, styled) clipboard when the system
        // clipboard still holds the same text we put there.
        let use_internal =
            !values_only && self.engine.has_internal_clip() && self.engine.clip_matches(&text);
        let cut_source = self.cut_marker.take();
        let result = if use_internal {
            self.engine.paste_internal(sheet, at, cut_source.is_some())
        } else if !text.is_empty() {
            self.engine.paste_text(sheet, at, &text)
        } else {
            Ok(())
        };
        if let Err(e) = result {
            self.set_status(format!("Paste: {e}"));
        }
        self.invalidate_geometry();
    }

    // ----- formatting helpers -----

    /// Applies a style edit to the current selection.
    pub fn style_selection(&mut self, edit: StyleEdit) {
        let sel = self.selection();
        let sheet = self.sheet;
        let result = self.engine.set_style(sheet, sel, edit);
        self.report("Format", result);
        self.invalidate_geometry();
    }

    /// Toggles a boolean font attribute based on the active cell's state.
    fn toggle_font(&mut self, which: Command) {
        let fmt = self.engine.format(self.sheet, self.cursor.0, self.cursor.1);
        let edit = match which {
            Command::ToggleBold => StyleEdit::Bold(!fmt.bold),
            Command::ToggleItalic => StyleEdit::Italic(!fmt.italic),
            Command::ToggleUnderline => StyleEdit::Underline(!fmt.underline),
            Command::ToggleStrike => StyleEdit::Strike(!fmt.strike),
            Command::ToggleWrap => StyleEdit::Wrap(!fmt.wrap),
            _ => return,
        };
        self.style_selection(edit);
    }

    fn change_font_size(&mut self, delta: i32) {
        let fmt = self.engine.format(self.sheet, self.cursor.0, self.cursor.1);
        let size = (fmt.font_size as i32 + delta).clamp(6, 96);
        self.style_selection(StyleEdit::FontSize(size));
    }

    /// Adds or removes one decimal place from the active cell's number format.
    fn change_decimals(&mut self, delta: i32) {
        let fmt = self.engine.format(self.sheet, self.cursor.0, self.cursor.1);
        let current = &fmt.num_fmt;
        let decimals = current
            .split_once('.')
            .map(|(_, rest)| rest.chars().take_while(|c| *c == '0').count() as i32)
            .unwrap_or(0);
        let new_decimals = (decimals + delta).clamp(0, 10);
        let has_comma = current.contains("#,##");
        let base = if has_comma { "#,##0" } else { "0" };
        let new_fmt = if new_decimals == 0 {
            base.to_string()
        } else {
            format!("{base}.{}", "0".repeat(new_decimals as usize))
        };
        self.style_selection(StyleEdit::NumFmt(new_fmt));
    }

    fn set_border(&mut self, target: BorderTarget) {
        let sel = self.selection();
        let sheet = self.sheet;
        let result = self.engine.set_border(sheet, sel, target, "#4A4A4A");
        self.report("Border", result);
    }

    // ----- structure -----

    fn insert_rows_at_selection(&mut self) {
        let sel = self.selection();
        let (sheet, count) = (self.sheet, sel.rows());
        let result = self.engine.insert_rows(sheet, sel.r0, count);
        self.report("Insert row", result);
        self.invalidate_geometry();
    }

    fn delete_rows_at_selection(&mut self) {
        let sel = self.selection();
        let (sheet, count) = (self.sheet, sel.rows());
        let result = self.engine.delete_rows(sheet, sel.r0, count);
        self.report("Delete row", result);
        self.invalidate_geometry();
    }

    fn insert_cols_at_selection(&mut self) {
        let sel = self.selection();
        let (sheet, count) = (self.sheet, sel.cols());
        let result = self.engine.insert_cols(sheet, sel.c0, count);
        self.report("Insert column", result);
        self.invalidate_geometry();
    }

    fn delete_cols_at_selection(&mut self) {
        let sel = self.selection();
        let (sheet, count) = (self.sheet, sel.cols());
        let result = self.engine.delete_cols(sheet, sel.c0, count);
        self.report("Delete column", result);
        self.invalidate_geometry();
    }

    fn autofit_columns(&mut self) {
        let sel = self.selection();
        let sheet = self.sheet;
        for c in sel.c0..=sel.c1 {
            let width = self.engine.autofit_width(sheet, c);
            let result = self.engine.set_col_width(sheet, c, c, width);
            self.report("Autofit", result);
        }
        self.invalidate_geometry();
    }

    // ----- fill -----

    pub fn fill_down(&mut self) {
        let sel = self.selection();
        let sheet = self.sheet;
        let result = if sel.rows() > 1 {
            let src = Range { r1: sel.r0, ..sel };
            self.engine.auto_fill_rows(sheet, src, sel.r1)
        } else if sel.r0 > 1 {
            let src = Range {
                r0: sel.r0 - 1,
                r1: sel.r0 - 1,
                ..sel
            };
            self.engine.auto_fill_rows(sheet, src, sel.r0)
        } else {
            Ok(())
        };
        self.report("Fill down", result);
    }

    pub fn fill_right(&mut self) {
        let sel = self.selection();
        let sheet = self.sheet;
        let result = if sel.cols() > 1 {
            let src = Range { c1: sel.c0, ..sel };
            self.engine.auto_fill_columns(sheet, src, sel.c1)
        } else if sel.c0 > 1 {
            let src = Range {
                c0: sel.c0 - 1,
                c1: sel.c0 - 1,
                ..sel
            };
            self.engine.auto_fill_columns(sheet, src, sel.c0)
        } else {
            Ok(())
        };
        self.report("Fill right", result);
    }

    // ----- find -----

    pub fn run_find(&mut self) {
        let (needle, case, workbook) = (
            self.find.needle.clone(),
            self.find.match_case,
            self.find.whole_workbook,
        );
        if needle.is_empty() {
            self.find.hits.clear();
            self.find.message.clear();
            return;
        }
        self.find.hits = self.engine.find(self.sheet, &needle, case, workbook);
        self.find.index = 0;
        self.find.message = if self.find.hits.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} match(es)", self.find.hits.len())
        };
        self.goto_hit();
    }

    pub fn find_step(&mut self, forward: bool) {
        if self.find.hits.is_empty() {
            self.run_find();
            return;
        }
        let len = self.find.hits.len();
        self.find.index = if forward {
            (self.find.index + 1) % len
        } else {
            (self.find.index + len - 1) % len
        };
        self.goto_hit();
    }

    fn goto_hit(&mut self) {
        let Some(hit) = self.find.hits.get(self.find.index).cloned() else {
            return;
        };
        if hit.sheet != self.sheet {
            self.switch_sheet(hit.sheet);
        }
        self.set_cursor(hit.row, hit.col);
        if !self.find.hits.is_empty() {
            self.find.message = format!("{} of {}", self.find.index + 1, self.find.hits.len());
        }
    }

    // ----- sort & filter -----

    fn sort_selection(&mut self, ascending: bool) {
        let mut sel = self.selection();
        // A single cell means "sort the surrounding data block by this column".
        let key_col = self.cursor.1;
        if sel.rows() == 1 && sel.cols() == 1 {
            let (ur, uc) = self.engine.used_range(self.sheet);
            sel = Range {
                r0: 1,
                r1: ur,
                c0: 1,
                c1: uc,
            };
            // Skip a header row if the first row looks like labels.
            if self.looks_like_header(sel) {
                sel.r0 = 2;
            }
        }
        let sheet = self.sheet;
        let result = self.engine.sort_range(sheet, sel, key_col, ascending);
        self.report("Sort", result);
    }

    fn looks_like_header(&self, range: Range) -> bool {
        (range.c0..=range.c1).any(|c| {
            !self.engine.is_empty(self.sheet, range.r0, c)
                && !self.engine.is_number(self.sheet, range.r0, c)
        })
    }

    /// Re-applies the active filter by hiding rows that fail it.
    pub fn apply_filter(&mut self) {
        let Some(filter) = self.filter.clone() else {
            return;
        };
        let sheet = self.sheet;
        for row in (filter.header_row + 1)..=filter.r1 {
            let mut visible = true;
            for (col, allowed) in &filter.allowed {
                if allowed.is_empty() {
                    continue;
                }
                let value = self.engine.display(sheet, row, *col);
                if !allowed.contains(&value) {
                    visible = false;
                    break;
                }
            }
            let _ = self.engine.set_rows_hidden(sheet, row, row, !visible);
        }
        self.invalidate_geometry();
    }

    fn toggle_filter(&mut self) {
        if let Some(filter) = self.filter.take() {
            let sheet = self.sheet;
            let _ = self
                .engine
                .set_rows_hidden(sheet, filter.header_row + 1, filter.r1, false);
            self.set_status("Filter removed");
        } else {
            let (ur, uc) = self.engine.used_range(self.sheet);
            let sel = self.selection();
            let (r0, r1, c0, c1) = if sel.rows() > 1 {
                (sel.r0, sel.r1, sel.c0, sel.c1)
            } else {
                (1, ur, 1, uc)
            };
            self.filter = Some(crate::state::Filter {
                header_row: r0,
                r0,
                r1,
                c0,
                c1,
                allowed: HashMap::new(),
                open_column: None,
            });
            self.set_status("Filter added — click a header arrow to filter");
        }
        self.invalidate_geometry();
    }

    // ----- sheets -----

    pub fn switch_sheet(&mut self, sheet: u32) {
        let count = self.engine.sheet_names().len() as u32;
        let sheet = sheet.min(count.saturating_sub(1));
        if sheet == self.sheet {
            return;
        }
        self.saved_cursors
            .insert(self.sheet, (self.cursor, self.anchor));
        self.sheet = sheet;
        let (cursor, anchor) = self
            .saved_cursors
            .get(&sheet)
            .copied()
            .unwrap_or(((1, 1), (1, 1)));
        self.cursor = cursor;
        self.anchor = anchor;
        self.edit = None;
        self.filter = None;
        self.reset_view();
        self.scroll_to_cursor = true;
    }

    // ----- file ops -----

    fn do_new(&mut self) {
        match Engine::new() {
            Ok(engine) => {
                self.engine = engine;
                self.reset_after_load();
                self.status.clear();
            }
            Err(e) => self.set_status(format!("New: {e}")),
        }
    }

    fn reset_after_load(&mut self) {
        self.sheet = 0;
        self.cursor = (1, 1);
        self.anchor = (1, 1);
        self.saved_cursors.clear();
        self.edit = None;
        self.filter = None;
        self.cut_marker = None;
        self.find.hits.clear();
        self.reset_view();
    }

    pub fn push_recent(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
        if let Some(file) = recent_file() {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let text: String = self
                .recent
                .iter()
                .map(|p| format!("{}\n", p.display()))
                .collect();
            let _ = std::fs::write(file, text);
        }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match Engine::open(&path) {
            Ok(engine) => {
                self.engine = engine;
                self.reset_after_load();
                self.set_status(format!("Opened {}", path.display()));
                self.push_recent(&path);
            }
            Err(e) => self.set_status(format!("Could not open {}: {e}", path.display())),
        }
    }

    fn do_open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Spreadsheet", &["xlsx"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    pub fn do_save(&mut self) -> bool {
        match self.engine.path.clone() {
            Some(path) => match self.engine.save(&path) {
                Ok(()) => {
                    self.set_status(format!("Saved {}", path.display()));
                    self.push_recent(&path);
                    true
                }
                Err(e) => {
                    self.set_status(format!("Save failed: {e}"));
                    false
                }
            },
            None => self.do_save_as(),
        }
    }

    pub fn do_save_as(&mut self) -> bool {
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("Spreadsheet", &["xlsx"])
            .set_file_name("Book1.xlsx")
            .save_file()
        else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("xlsx");
        }
        match self.engine.save(&path) {
            Ok(()) => {
                self.set_status(format!("Saved {}", path.display()));
                self.push_recent(&path);
                true
            }
            Err(e) => {
                self.set_status(format!("Save failed: {e}"));
                false
            }
        }
    }

    fn request(&mut self, action: Pending, ctx: &Context) {
        if self.engine.dirty {
            self.pending = Some(action);
        } else {
            self.run_pending(action, ctx);
        }
    }

    pub fn run_pending(&mut self, action: Pending, ctx: &Context) {
        match action {
            Pending::New => self.do_new(),
            Pending::Open => self.do_open(),
            Pending::OpenPath(path) => self.open_path(path),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    // ----- dispatch -----

    pub fn exec(&mut self, cmd: Command, ctx: &Context) {
        use Command::*;
        match cmd {
            MoveUp => self.move_cursor(-1, 0, false),
            MoveDown => self.move_cursor(1, 0, false),
            MoveLeft => self.move_cursor(0, -1, false),
            MoveRight => self.move_cursor(0, 1, false),
            ExtendUp => self.move_cursor(-1, 0, true),
            ExtendDown => self.move_cursor(1, 0, true),
            ExtendLeft => self.move_cursor(0, -1, true),
            ExtendRight => self.move_cursor(0, 1, true),
            JumpUp => self.jump(-1, 0, false),
            JumpDown => self.jump(1, 0, false),
            JumpLeft => self.jump(0, -1, false),
            JumpRight => self.jump(0, 1, false),
            ExtendJumpUp => self.jump(-1, 0, true),
            ExtendJumpDown => self.jump(1, 0, true),
            ExtendJumpLeft => self.jump(0, -1, true),
            ExtendJumpRight => self.jump(0, 1, true),
            PageUp => self.move_cursor(-self.rows_per_page.max(1), 0, false),
            PageDown => self.move_cursor(self.rows_per_page.max(1), 0, false),
            Home => self.set_cursor(self.cursor.0, 1),
            HomeAll => self.set_cursor(1, 1),
            EndData => {
                let (ur, uc) = self.engine.used_range(self.sheet);
                self.set_cursor(ur, uc);
            }
            GotoCell => {
                self.goto_text.clear();
                self.dialog = Dialog::GotoCell;
            }

            SelectAll => {
                let (ur, uc) = self.engine.used_range(self.sheet);
                self.anchor = (1, 1);
                self.cursor = (ur.max(1), uc.max(1));
            }
            SelectRow => {
                let row = self.cursor.0;
                self.anchor = (row, 1);
                self.cursor = (row, self.view_cols.min(MAX_COLS));
            }
            SelectColumn => {
                let col = self.cursor.1;
                self.anchor = (1, col);
                self.cursor = (self.view_rows.min(MAX_ROWS), col);
            }

            EditCell => {
                let text = self.engine.content(self.sheet, self.cursor.0, self.cursor.1);
                self.begin_edit(text, EditMode::Edit);
            }
            Clear => {
                let sel = self.selection();
                let sheet = self.sheet;
                let result = self.engine.clear_contents(sheet, sel);
                self.report("Clear", result);
            }
            ClearFormats => {
                let sel = self.selection();
                let sheet = self.sheet;
                let result = self.engine.clear_formatting(sheet, sel);
                self.report("Clear formats", result);
                self.invalidate_geometry();
            }
            ClearAll => {
                let sel = self.selection();
                let sheet = self.sheet;
                let result = self.engine.clear_all(sheet, sel);
                self.report("Clear all", result);
                self.invalidate_geometry();
            }
            FillDown => self.fill_down(),
            FillRight => self.fill_right(),
            Undo => {
                self.engine.undo();
                self.invalidate_geometry();
            }
            Redo => {
                self.engine.redo();
                self.invalidate_geometry();
            }
            Copy => self.copy_selection(ctx, false),
            Cut => self.copy_selection(ctx, true),
            Paste => self.paste(false),
            PasteValues => self.paste(true),

            ToggleBold | ToggleItalic | ToggleUnderline | ToggleStrike | ToggleWrap => {
                self.toggle_font(cmd)
            }
            AlignLeft => self.style_selection(StyleEdit::Align(HAlign::Left)),
            AlignCenter => self.style_selection(StyleEdit::Align(HAlign::Center)),
            AlignRight => self.style_selection(StyleEdit::Align(HAlign::Right)),
            IncreaseFont => self.change_font_size(1),
            DecreaseFont => self.change_font_size(-1),
            BorderAll => self.set_border(BorderTarget::All),
            BorderOuter => self.set_border(BorderTarget::Outer),
            BorderBottom => self.set_border(BorderTarget::Bottom),
            BorderNone => self.set_border(BorderTarget::None),
            FormatGeneral => self.style_selection(StyleEdit::NumFmt("general".into())),
            FormatNumber => self.style_selection(StyleEdit::NumFmt("0.00".into())),
            FormatCurrency => {
                self.style_selection(StyleEdit::NumFmt("\"$\"#,##0.00".into()))
            }
            FormatPercent => self.style_selection(StyleEdit::NumFmt("0.00%".into())),
            FormatComma => self.style_selection(StyleEdit::NumFmt("#,##0.00".into())),
            FormatDate => self.style_selection(StyleEdit::NumFmt("yyyy-mm-dd".into())),
            IncreaseDecimal => self.change_decimals(1),
            DecreaseDecimal => self.change_decimals(-1),
            FormatDialog => self.dialog = Dialog::FormatCells,

            InsertRow => self.insert_rows_at_selection(),
            InsertColumn => self.insert_cols_at_selection(),
            DeleteRow => self.delete_rows_at_selection(),
            DeleteColumn => self.delete_cols_at_selection(),
            AutofitColumn => self.autofit_columns(),

            FindReplace => {
                self.dialog = Dialog::FindReplace;
            }
            FindNext => self.find_step(true),
            FindPrev => self.find_step(false),
            SortAscending => self.sort_selection(true),
            SortDescending => self.sort_selection(false),
            ToggleFilter => self.toggle_filter(),
            NameManager => self.dialog = Dialog::NameManager,
            ConditionalFormat => self.dialog = Dialog::ConditionalFormat,
            InsertChart => self.dialog = Dialog::Chart,

            FreezePanes => {
                let (r, c) = (self.cursor.0 - 1, self.cursor.1 - 1);
                let sheet = self.sheet;
                let result = self.engine.set_frozen(sheet, r, c);
                self.report("Freeze", result);
            }
            FreezeTopRow => {
                let sheet = self.sheet;
                let result = self.engine.set_frozen(sheet, 1, 0);
                self.report("Freeze", result);
            }
            FreezeFirstColumn => {
                let sheet = self.sheet;
                let result = self.engine.set_frozen(sheet, 0, 1);
                self.report("Freeze", result);
            }
            Unfreeze => {
                let sheet = self.sheet;
                let result = self.engine.set_frozen(sheet, 0, 0);
                self.report("Unfreeze", result);
            }
            ToggleGridLines => {
                let sheet = self.sheet;
                let show = !self.engine.show_grid_lines(sheet);
                let result = self.engine.set_show_grid_lines(sheet, show);
                self.report("Grid lines", result);
            }
            ZoomIn => {
                self.zoom = (self.zoom + 0.1).min(4.0);
                self.invalidate_geometry();
            }
            ZoomOut => {
                self.zoom = (self.zoom - 0.1).max(0.4);
                self.invalidate_geometry();
            }
            ZoomReset => {
                self.zoom = 1.0;
                self.invalidate_geometry();
            }

            New => self.request(Pending::New, ctx),
            Open => self.request(Pending::Open, ctx),
            Save => {
                self.do_save();
            }
            SaveAs => {
                self.do_save_as();
            }
            Quit => self.request(Pending::Quit, ctx),

            NextSheet => self.switch_sheet(self.sheet + 1),
            PrevSheet => self.switch_sheet(self.sheet.saturating_sub(1)),
            NewSheet => {
                if let Err(e) = self.engine.new_sheet() {
                    self.set_status(format!("New sheet: {e}"));
                } else {
                    let last = self.engine.sheet_names().len() as u32 - 1;
                    self.switch_sheet(last);
                }
            }
            RenameSheet => {
                let name = self
                    .engine
                    .sheet_names()
                    .get(self.sheet as usize)
                    .cloned()
                    .unwrap_or_default();
                self.dialog = Dialog::RenameSheet {
                    sheet: self.sheet,
                    name,
                };
            }
            DeleteSheet => {
                if self.engine.sheet_names().len() <= 1 {
                    self.set_status("A workbook needs at least one sheet");
                } else {
                    let sheet = self.sheet;
                    let result = self.engine.delete_sheet(sheet);
                    self.report("Delete sheet", result);
                    self.sheet = 0;
                    self.cursor = (1, 1);
                    self.anchor = (1, 1);
                    self.reset_view();
                }
            }
            DuplicateSheet => {
                let sheet = self.sheet;
                let result = self.engine.duplicate_sheet(sheet);
                self.report("Duplicate sheet", result);
            }
            MoveSheetLeft => {
                if self.sheet > 0 {
                    let (from, to) = (self.sheet, self.sheet - 1);
                    let result = self.engine.move_sheet(from, to);
                    self.report("Move sheet", result);
                    self.sheet = to;
                }
            }
            MoveSheetRight => {
                let count = self.engine.sheet_names().len() as u32;
                if self.sheet + 1 < count {
                    let (from, to) = (self.sheet, self.sheet + 1);
                    let result = self.engine.move_sheet(from, to);
                    self.report("Move sheet", result);
                    self.sheet = to;
                }
            }

            ShortcutHelp => self.dialog = Dialog::Shortcuts,
        }
    }

    // ----- input -----

    fn collect_input(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        // While a text field (cell editor, dialog, ...) has focus, global
        // shortcuts stay out of the way.
        if self.edit.is_some() || self.dialog != Dialog::None || ctx.wants_keyboard_input() {
            return cmds;
        }
        ctx.input_mut(|input| {
            for (shortcut, cmd) in &self.keymap.bindings {
                if input.consume_shortcut(shortcut) {
                    cmds.push(*cmd);
                }
            }
        });
        let mut typed = String::new();
        let mut clipboard = Vec::new();
        ctx.input(|input| {
            for event in &input.events {
                match event {
                    Event::Text(t) => typed.push_str(t),
                    Event::Copy => clipboard.push(Command::Copy),
                    Event::Cut => clipboard.push(Command::Cut),
                    Event::Paste(_) => {
                        clipboard.push(Command::Paste);
                        typed.clear();
                    }
                    _ => {}
                }
            }
        });
        for cmd in clipboard {
            if !cmds.contains(&cmd) {
                cmds.push(cmd);
            }
        }
        // Ctrl+scroll zooms.
        let (scroll, ctrl) = ctx.input(|i| (i.raw_scroll_delta.y, i.modifiers.ctrl));
        if ctrl && scroll.abs() > 0.5 {
            cmds.push(if scroll > 0.0 {
                Command::ZoomIn
            } else {
                Command::ZoomOut
            });
        }
        // Typing a printable character starts editing the active cell,
        // replacing its content — "Enter mode".
        if !typed.is_empty() && !typed.chars().all(char::is_control) {
            self.begin_edit(typed, EditMode::Enter);
        }
        cmds
    }

    // ----- editing entry points (shared with ui::editor) -----

    pub fn begin_edit(&mut self, text: String, mode: EditMode) {
        self.anchor = self.cursor;
        self.edit = Some(EditState::new(
            self.cursor.0,
            self.cursor.1,
            text,
            mode,
            false,
        ));
    }

    pub fn commit_edit(&mut self, row: i32, col: i32, text: &str) {
        let sheet = self.sheet;
        let result = self.engine.set_input(sheet, row, col, text.trim_end());
        self.report("Edit", result);
        self.invalidate_geometry();
    }


    /// Runs any assistant tool calls waiting on the bridge.
    ///
    /// This happens on the UI thread, between frames, so a tool sees a
    /// consistent workbook and the user sees the result immediately.
    fn drain_assistant_calls(&mut self) {
        // Refuse while a cell editor is open rather than yanking it away.
        let busy = self.edit.is_some();
        let mut calls = Vec::new();
        if let Some(rx) = &self.mcp_rx {
            while let Ok(call) = rx.try_recv() {
                calls.push(call);
            }
        }
        for call in calls {
            if busy {
                let _ = call.reply.send(Err(
                    "the user is editing a cell right now — try again in a moment".to_string(),
                ));
                continue;
            }
            // Back the file up once per session before the first write.
            if !self.backed_up && mutates(&call.tool) {
                self.backup_once();
            }
            let result = crate::mcp::tools::dispatch(self, &call.tool, &call.args)
                .map_err(|e| format!("{e:#}"));
            // The person this is built for will not think to press Ctrl+S, so
            // an assistant edit schedules its own save once things go quiet.
            if result.is_ok() && mutates(&call.tool) && self.engine.path.is_some() {
                self.autosave_in = Some(3.0);
            }
            match &result {
                Ok(_) => self.log_activity(describe(&call.tool, &call.args), true),
                Err(e) => self.log_activity(format!("{}: {e}", call.tool), false),
            }
            let _ = call.reply.send(result);
        }
    }

    /// Copies the workbook to `<name>.bak.xlsx` before the assistant's first
    /// write of the session. Cheap insurance for a file kept for months.
    fn backup_once(&mut self) {
        self.backed_up = true;
        let Some(path) = self.engine.path.clone() else {
            return;
        };
        let backup = path.with_extension("bak.xlsx");
        if let Err(e) = std::fs::copy(&path, &backup) {
            self.set_status(format!("Backup failed: {e}"));
        }
    }

    /// Saves a few seconds after the assistant stops editing, so a burst of
    /// tool calls produces one save rather than one per call.
    fn tick_autosave(&mut self, ctx: &Context) {
        let Some(remaining) = self.autosave_in else {
            return;
        };
        let dt = ctx.input(|i| i.stable_dt).min(0.5);
        let remaining = remaining - dt;
        if remaining > 0.0 {
            self.autosave_in = Some(remaining);
            ctx.request_repaint();
            return;
        }
        self.autosave_in = None;
        if let Some(path) = self.engine.path.clone() {
            match self.engine.save(&path) {
                Ok(()) => self.set_status(format!("Saved {}", path.display())),
                Err(e) => self.set_status(format!("Autosave failed: {e}")),
            }
        }
    }

    /// Fades assistant change highlights, and keeps repainting while any are
    /// still visible.
    fn tick_change_highlights(&mut self, ctx: &Context) {
        if self.recent_changes.is_empty() {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        for change in &mut self.recent_changes {
            change.ttl -= dt;
        }
        self.recent_changes.retain(|c| c.ttl > 0.0);
        ctx.request_repaint();
    }

    /// Modal for a destructive action the assistant asked to perform.
    fn assistant_confirm(&mut self, ctx: &Context) {
        let Some(request) = self.assistant_request.clone() else {
            return;
        };
        let mut choice = None;
        eframe::egui::Window::new("Assistant request")
            .collapsible(false)
            .resizable(false)
            .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&request.prompt);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Allow").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Refuse").clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                self.assistant_request = None;
                match request.action {
                    crate::state::AssistantAction::DeleteRows { sheet, at, count } => {
                        let result = self.engine.delete_rows(sheet, at, count);
                        self.report("Delete rows", result);
                        self.invalidate_geometry();
                    }
                }
            }
            Some(false) => {
                self.assistant_request = None;
                self.set_status("Assistant request refused");
            }
            None => {}
        }
    }
}

impl eframe::App for GridoApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Restyle in place when the Omarchy theme changes under us.
        if let Some(palette) = self.theme_watcher.changed(ctx) {
            ctx.set_visuals(enclave_ui::theme::visuals(&palette));
        }
        self.drain_assistant_calls();
        self.tick_autosave(ctx);
        self.tick_change_highlights(ctx);

        // Intercept window close while there are unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && self.engine.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }

        if self.backstage {
            let cmds = self.backstage_ui(ctx);
            for cmd in cmds {
                self.exec(cmd, ctx);
            }
            self.confirm_dialog(ctx);
            self.update_title(ctx);
            return;
        }

        let mut cmds = self.collect_input(ctx);
        cmds.extend(self.ribbon(ctx));
        cmds.extend(self.dialogs(ctx));

        self.formula_bar(ctx);
        self.status_bar(ctx);
        self.sheet_tabs(ctx);
        cmds.extend(self.grid(ctx));

        for cmd in cmds {
            self.exec(cmd, ctx);
        }

        self.confirm_dialog(ctx);
        self.assistant_confirm(ctx);
        self.update_title(ctx);
    }
}

impl GridoApp {
    fn update_title(&mut self, ctx: &Context) {
        let name = self
            .engine
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Book1".to_string());
        let title = format!(
            "Grido — {}{}",
            name,
            if self.engine.dirty { " •" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

/// Tools that change the workbook, rather than only look at it.
///
/// Two callers: the backup taken before the assistant's first write, and the
/// host's confirmation broker, which asks the user before anything here runs.
/// A name this list has never heard of counts as a change — the safe way for
/// the answer to be wrong.
pub fn mutates(tool: &str) -> bool {
    !matches!(
        tool,
        "grido_workbook_info"
            | "grido_range_read"
            | "grido_table_schema"
            | "grido_table_find"
            | "grido_find"
            | "grido_stats"
    )
}

/// A one-line, plain-English account of a tool call — for the activity log,
/// and for the sentence the confirmation window asks about.
pub fn describe(tool: &str, args: &serde_json::Value) -> String {
    let get = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str().map(str::to_string).or_else(|| Some(v.to_string())))
            .unwrap_or_default()
    };
    match tool {
        "grido_cell_set" => format!("set {} to {}", get("cell"), get("value")),
        "grido_range_set" => format!("wrote a block at {}", get("range")),
        "grido_range_format" => format!("formatted {}", get("range")),
        "grido_table_append" => format!("added a row: {}", get("record")),
        "grido_table_update" => format!("updated rows matching {}", get("criteria")),
        "grido_rows_insert" => format!("inserted rows at {}", get("at")),
        "grido_rows_delete" => format!("asked to delete rows at {}", get("at")),
        "grido_sheet_add" => "added a sheet".to_string(),
        "grido_sheet_rename" => format!("renamed a sheet to {}", get("name")),
        "grido_sort" => format!("sorted {} by {}", get("range"), get("by")),
        "grido_workbook_open" => format!("opened {}", get("path")),
        "grido_workbook_save" => "saved the workbook".to_string(),
        "grido_template_create" => format!("created a {} workbook", get("kind")),
        "grido_undo" => "undid the last action".to_string(),
        other => other.to_string(),
    }
}

/// One axis of the Ctrl+Arrow "jump to data boundary" move.
/// `far` is the sheet edge in that direction; `scan_end` is the last position
/// worth scanning (the used range).
fn jump_axis(start: i32, dir: i32, far: i32, scan_end: i32, filled: impl Fn(i32) -> bool) -> i32 {
    let next = start + dir;
    let in_range = |i: i32| {
        if dir > 0 {
            i <= scan_end.max(start)
        } else {
            i >= scan_end
        }
    };
    if !in_range(next) || next < 1 || next > far.max(1) {
        return far;
    }
    if filled(start) && filled(next) {
        let mut i = next;
        while in_range(i + dir) && filled(i + dir) {
            i += dir;
        }
        i
    } else {
        let mut i = next;
        while in_range(i) {
            if filled(i) {
                return i;
            }
            i += dir;
        }
        far
    }
}

#[cfg(test)]
mod tests {
    use super::jump_axis;

    #[test]
    fn jump_from_filled_block_stops_at_its_end() {
        // Cells 1..3 filled, 4+ empty. From 1 going down we land on 3.
        let filled = |i: i32| (1..=3).contains(&i);
        assert_eq!(jump_axis(1, 1, 1_048_576, 3, filled), 3);
    }

    #[test]
    fn jump_over_blanks_lands_on_next_filled() {
        // 1 filled, 2-3 blank, 4 filled.
        let filled = |i: i32| i == 1 || i == 4;
        assert_eq!(jump_axis(1, 1, 1_048_576, 4, filled), 4);
    }

    #[test]
    fn jump_with_no_data_hits_the_sheet_edge() {
        assert_eq!(jump_axis(5, 1, 1_048_576, 1, |_| false), 1_048_576);
        assert_eq!(jump_axis(5, -1, 1, 1, |_| false), 1);
    }
}
