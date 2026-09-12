//! Formula bar, name box and the in-cell editor.

use std::cell::Cell;

use eframe::egui::{
    self, Context, FontId, Id, Key, Modifiers, Order, Pos2, RichText, TextEdit, Vec2,
};

use crate::engine::{parse_cell_ref, Range};
use crate::state::{EditMode, EditState, GridoApp, BASE_FONT};
use crate::ui::grid::{column_name, range_name};

/// Fixed widget ids, so the caret can be read and written without holding on
/// to the widget's `Response`.
const CELL_EDIT_ID: &str = "sheetz-cell-edit-text";
const BAR_EDIT_ID: &str = "sheetz-formula-bar-text";

/// A clicked cell becomes a reference when the caret follows one of these.
const REF_OPERATORS: &str = "=+-*/(,^&<>%:";

/// Colors handed out to the distinct references of the formula being edited.
const REF_COLORS: [[u8; 3]; 5] = [
    [0, 112, 192],
    [192, 0, 0],
    [0, 128, 0],
    [128, 0, 128],
    [214, 112, 0],
];

thread_local! {
    /// Caret of the active editor, refreshed every frame. The grid asks about
    /// it (click-to-reference) without a `Context`, so it cannot read the
    /// `TextEdit` state itself.
    static CARET: Cell<usize> = const { Cell::new(0) };
    /// Caret to force into the editor next time it is drawn.
    static PENDING_CARET: Cell<Option<usize>> = const { Cell::new(None) };
}

fn caret_pos() -> usize {
    CARET.with(|c| c.get())
}

/// Queues a caret position for the next time the editor is drawn. Used from
/// the grid, which has no `Context` to write the widget state with.
fn set_pending_caret(pos: usize) {
    CARET.with(|c| c.set(pos));
    PENDING_CARET.with(|c| c.set(Some(pos)));
}

impl GridoApp {
    pub fn formula_bar(&mut self, ctx: &Context) {
        let mut commit: Option<Option<(i32, i32)>> = None;
        let mut cancel = false;

        egui::TopBottomPanel::top("formulabar").show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // Name box — type a reference to jump there.
                let name = range_name(self.selection());
                let resp = ui.add_sized(
                    [90.0, 22.0],
                    TextEdit::singleline(&mut self.goto_text)
                        .hint_text(name)
                        .font(FontId::monospace(12.0)),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if let Some((row, col)) = parse_cell_ref(&self.goto_text) {
                        self.set_cursor(row, col);
                    } else {
                        self.set_status(format!("Bad reference: {}", self.goto_text));
                    }
                    self.goto_text.clear();
                }
                ui.separator();

                let editing_here = self
                    .edit
                    .as_ref()
                    .map(|e| e.in_formula_bar)
                    .unwrap_or(false);

                // Cancel / enter buttons, between the name box and the formula.
                if ui
                    .add_enabled(editing_here, egui::Button::new("✗"))
                    .on_hover_text("Cancel")
                    .clicked()
                {
                    cancel = true;
                }
                if ui
                    .add_enabled(editing_here, egui::Button::new("✓"))
                    .on_hover_text("Enter")
                    .clicked()
                {
                    commit = Some(None);
                }

                // The grid handles clicks after this panel is drawn, so a click
                // that is about to become a reference must not commit here.
                let mut keep_focus = self.editing_formula_wants_reference();
                let mut popup = false;
                if editing_here {
                    keep_focus |= self.apply_pending_caret(ctx);
                    self.handle_f4(ctx);
                    popup = self.completion_keys(ctx);
                    // The bar is multiline, so Enter would insert a newline
                    // unless we take it first — except with Alt, which is the
                    // line-break chord.
                    let (shift, alt) = ctx.input(|i| (i.modifiers.shift, i.modifiers.alt));
                    if !popup && !alt {
                        ctx.input_mut(|i| {
                            if i.consume_key(Modifiers::NONE, Key::Escape) {
                                cancel = true;
                            } else if i.consume_key(Modifiers::NONE, Key::Enter) {
                                commit = Some(Some(if shift { (-1, 0) } else { (1, 0) }));
                            } else if i.consume_key(Modifiers::NONE, Key::Tab) {
                                commit = Some(Some(if shift { (0, -1) } else { (0, 1) }));
                            }
                        });
                    }
                }

                let mut shown = match &self.edit {
                    Some(edit) => edit.text.clone(),
                    None => self.engine.content(self.sheet, self.cursor.0, self.cursor.1),
                };
                let rows = bar_rows(&shown);
                let resp = ui.add(
                    TextEdit::multiline(&mut shown)
                        .id(Id::new(BAR_EDIT_ID))
                        .font(FontId::monospace(BASE_FONT))
                        .desired_rows(rows)
                        .desired_width(ui.available_width()),
                );
                if resp.gained_focus() && self.edit.is_none() {
                    self.anchor = self.cursor;
                    self.edit = Some(EditState::new(
                        self.cursor.0,
                        self.cursor.1,
                        shown.clone(),
                        EditMode::Edit,
                        true,
                    ));
                }
                if editing_here {
                    if let Some(edit) = &mut self.edit {
                        edit.text = shown;
                    }
                    if resp.lost_focus() && commit.is_none() && !cancel {
                        if keep_focus {
                            resp.request_focus();
                        } else {
                            commit = Some(None);
                        }
                    }
                    if popup {
                        self.completion_popup(ctx, resp.rect.left_bottom(), resp.rect.width());
                    }
                    self.store_caret(ctx);
                }
            });
        });

        if cancel || commit.is_some() {
            // Keys we swallowed above would normally have taken focus off the
            // bar; hand it back to the grid so typing goes there again.
            ctx.memory_mut(|m| m.surrender_focus(Id::new(BAR_EDIT_ID)));
        }
        if cancel {
            self.edit = None;
        } else if let Some(move_after) = commit {
            if let Some(edit) = self.edit.take() {
                self.commit_edit(edit.row, edit.col, &edit.text);
            }
            if let Some((dr, dc)) = move_after {
                self.move_cursor(dr, dc, false);
            }
        }
    }

    pub fn edit_overlay(&mut self, ctx: &Context, grid_rect: egui::Rect, offset: Vec2) {
        let Some(edit) = &self.edit else { return };
        if edit.in_formula_bar {
            return;
        }
        let (row, col) = (edit.row, edit.col);
        let (r, c) = (row as usize, col as usize);
        if r >= self.row_off.len() || c >= self.col_off.len() {
            return;
        }
        let min = Pos2::new(
            grid_rect.min.x + self.col_off[c - 1] - offset.x,
            grid_rect.min.y + self.row_off[r - 1] - offset.y,
        );
        let size = Vec2::new(
            (self.col_off[c] - self.col_off[c - 1]).max(120.0),
            (self.row_off[r] - self.row_off[r - 1]).max(20.0),
        );

        let refocus = self.apply_pending_caret(ctx);
        self.handle_f4(ctx);
        let popup = self.completion_keys(ctx);

        let mut committed = false;
        let mut cancelled = false;
        let mut move_after: Option<(i32, i32)> = None;

        egui::Area::new(Id::new("sheetz-cell-edit"))
            .fixed_pos(min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let edit = self.edit.as_mut().unwrap();
                let resp = ui.add(
                    TextEdit::singleline(&mut edit.text)
                        .id(Id::new(CELL_EDIT_ID))
                        .font(FontId::proportional(BASE_FONT))
                        .min_size(size)
                        .vertical_align(egui::Align::Center),
                );
                if edit.take_focus {
                    resp.request_focus();
                    if edit.caret_end {
                        let id = resp.id;
                        if let Some(mut state) = TextEdit::load_state(ctx, id) {
                            let end = egui::text::CCursor::new(edit.text.chars().count());
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(end)));
                            state.store(ctx, id);
                        }
                    }
                    edit.take_focus = false;
                }
                if refocus {
                    // A grid click inserted a reference and stole focus.
                    resp.request_focus();
                }

                // In Enter mode (started by typing), arrows commit and move.
                // In Edit mode (F2) they move the caret.
                // While the completion list is up it owns the arrows instead.
                if edit.mode == EditMode::Enter && !popup {
                    let (up, down, left, right) = ui.input(|i| {
                        (
                            i.key_pressed(Key::ArrowUp),
                            i.key_pressed(Key::ArrowDown),
                            i.key_pressed(Key::ArrowLeft),
                            i.key_pressed(Key::ArrowRight),
                        )
                    });
                    if up || down || left || right {
                        committed = true;
                        move_after = Some(if up {
                            (-1, 0)
                        } else if down {
                            (1, 0)
                        } else if left {
                            (0, -1)
                        } else {
                            (0, 1)
                        });
                    }
                }

                if resp.lost_focus() && !committed && !refocus {
                    let (esc, enter, tab, shift) = ui.input(|i| {
                        (
                            i.key_pressed(Key::Escape),
                            i.key_pressed(Key::Enter),
                            i.key_pressed(Key::Tab),
                            i.modifiers.shift,
                        )
                    });
                    if esc {
                        cancelled = true;
                    } else {
                        committed = true;
                        if enter {
                            move_after = Some(if shift { (-1, 0) } else { (1, 0) });
                        } else if tab {
                            move_after = Some(if shift { (0, -1) } else { (0, 1) });
                        }
                    }
                }
            });

        if popup && !committed && !cancelled {
            self.completion_popup(ctx, min + Vec2::new(0.0, size.y), size.x);
        }
        self.store_caret(ctx);

        if committed || cancelled {
            let edit = self.edit.take().unwrap();
            if committed {
                self.commit_edit(edit.row, edit.col, &edit.text);
            }
            if let Some((dr, dc)) = move_after {
                self.move_cursor(dr, dc, false);
            }
        }
    }

    // ----- caret plumbing -----

    fn edit_id(&self) -> Option<Id> {
        let edit = self.edit.as_ref()?;
        Some(Id::new(if edit.in_formula_bar {
            BAR_EDIT_ID
        } else {
            CELL_EDIT_ID
        }))
    }

    /// Caret char range of the active editor, as (start, end).
    fn caret_range(&self, ctx: &Context) -> (usize, usize) {
        let Some(id) = self.edit_id() else {
            return (0, 0);
        };
        let Some(state) = TextEdit::load_state(ctx, id) else {
            return (0, 0);
        };
        match state.cursor.char_range() {
            Some(range) => {
                let [a, b] = range.sorted_cursors();
                (a.index, b.index)
            }
            None => (0, 0),
        }
    }

    fn store_caret(&self, ctx: &Context) {
        let len = self.edit.as_ref().map(|e| e.text.chars().count()).unwrap_or(0);
        let (_, end) = self.caret_range(ctx);
        CARET.with(|c| c.set(end.min(len)));
    }

    fn set_caret(&self, ctx: &Context, pos: usize) {
        let Some(id) = self.edit_id() else { return };
        let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
        let cursor = egui::text::CCursor::new(pos);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
        state.store(ctx, id);
    }

    /// Moves the caret now — only valid before the editor is drawn this frame.
    fn place_caret(&self, ctx: &Context, pos: usize) {
        CARET.with(|c| c.set(pos));
        self.set_caret(ctx, pos);
    }

    /// Applies a caret position queued by F4 / completion / click-to-reference.
    /// Returns true if one was pending, which also means focus may need to be
    /// taken back from whatever consumed the click.
    fn apply_pending_caret(&mut self, ctx: &Context) -> bool {
        let Some(pos) = PENDING_CARET.with(|c| c.take()) else {
            return false;
        };
        self.set_caret(ctx, pos);
        true
    }

    // ----- F4 -----

    fn handle_f4(&mut self, ctx: &Context) {
        // Plain F4 only: consume_key ignores extra Alt/Shift, and Alt+F4 is the
        // window manager's.
        if ctx.input(|i| i.modifiers.any()) {
            return;
        }
        if !ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F4)) {
            return;
        }
        let (start, end) = self.caret_range(ctx);
        let Some(edit) = &self.edit else { return };
        let text = edit.text.clone();
        let Some((new_text, caret)) = self.engine.cycle_reference(&text, start, end) else {
            return;
        };
        let caret = caret.min(new_text.chars().count());
        self.edit.as_mut().unwrap().text = new_text;
        self.place_caret(ctx, caret);
    }

    // ----- completions -----

    /// Refreshes the cached list, handles the popup's keys and reports whether
    /// the popup is showing. Must run before the editor's `TextEdit`, so the
    /// keys it owns never reach the text field.
    fn completion_keys(&mut self, ctx: &Context) -> bool {
        let caret = caret_pos();
        self.refresh_completions();
        let Some(edit) = &self.edit else { return false };
        let items = visible_completions(edit, caret);
        if items.is_empty() {
            return false;
        }
        let index = edit.completion_index.min(items.len() - 1);

        let (up, down, accept, dismiss) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Tab)
                    || i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });

        let edit = self.edit.as_mut().unwrap();
        if dismiss {
            // Only the popup goes away; the edit continues.
            edit.completions.clear();
            edit.completions_for = edit.text.clone();
            return false;
        }
        if up {
            edit.completion_index = if index == 0 { items.len() - 1 } else { index - 1 };
            return true;
        }
        if down {
            edit.completion_index = (index + 1) % items.len();
            return true;
        }
        edit.completion_index = index;
        if accept {
            let (text, caret) = accept_completion(&edit.text, caret, &items[index]);
            edit.text = text;
            edit.completions.clear();
            edit.completions_for = edit.text.clone();
            self.place_caret(ctx, caret);
            return false;
        }
        true
    }

    fn refresh_completions(&mut self) {
        let Some(edit) = &self.edit else { return };
        if edit.completions_for == edit.text {
            return;
        }
        let (text, row, col) = (edit.text.clone(), edit.row, edit.col);
        let mut list = Vec::new();
        if text.starts_with('=') {
            let sheet = self.sheet;
            list = self.engine.completions(sheet, row, col, &text);
            for name in &mut list {
                *name = name.trim_end_matches('(').to_string();
            }
            list.retain(|name| !name.is_empty());
        }
        let edit = self.edit.as_mut().unwrap();
        edit.completions = list;
        edit.completions_for = text;
        edit.completion_index = 0;
    }

    fn completion_popup(&mut self, ctx: &Context, at: Pos2, width: f32) {
        let caret = caret_pos();
        let Some(edit) = &self.edit else { return };
        let items = visible_completions(edit, caret);
        if items.is_empty() {
            return;
        }
        let index = edit.completion_index.min(items.len() - 1);
        egui::Area::new(Id::new("sheetz-completions"))
            .fixed_pos(at)
            .order(Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(width.max(140.0));
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for (i, name) in items.iter().enumerate() {
                        let mut text = RichText::new(name).font(FontId::monospace(12.0));
                        if i == index {
                            text = text
                                .background_color(ui.visuals().selection.bg_fill)
                                .color(ui.visuals().strong_text_color());
                        }
                        ui.label(text);
                    }
                });
            });
    }

    // ----- click-to-reference (called by the grid) -----

    /// True when a grid click should become a reference instead of moving the
    /// selection.
    pub fn editing_formula_wants_reference(&self) -> bool {
        let Some(edit) = &self.edit else { return false };
        edit.is_formula() && reference_slot(&edit.text, caret_pos()).is_some()
    }

    /// Inserts (or, in point mode, replaces) a reference to the clicked
    /// cell. Returns true if the click was consumed.
    pub fn insert_reference_at_caret(&mut self, row: i32, col: i32) -> bool {
        let reference = format!("{}{}", column_name(col), row);
        let Some(edit) = &mut self.edit else {
            return false;
        };
        if !edit.is_formula() {
            return false;
        }
        let caret = caret_pos();
        let Some((start, end)) = reference_slot(&edit.text, caret) else {
            return false;
        };
        let chars: Vec<char> = edit.text.chars().collect();
        let mut out: String = chars[..start].iter().collect();
        out.push_str(&reference);
        out.extend(chars[end..].iter());
        edit.text = out;
        // No function is being typed any more.
        edit.completions.clear();
        edit.completions_for = edit.text.clone();
        set_pending_caret(start + reference.chars().count());
        true
    }

    /// References in the formula being edited, each with the color the grid
    /// should outline it in.
    pub fn formula_reference_ranges(&self) -> Vec<(Range, [u8; 3])> {
        let Some(edit) = &self.edit else {
            return Vec::new();
        };
        if !edit.is_formula() {
            return Vec::new();
        }
        let mut seen: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for token in scan_references(&edit.text) {
            let Some(range) = parse_reference(&token) else {
                continue;
            };
            let key = token.replace('$', "").to_ascii_uppercase();
            let slot = match seen.iter().position(|s| *s == key) {
                Some(i) => i,
                None => {
                    seen.push(key);
                    seen.len() - 1
                }
            };
            out.push((range, REF_COLORS[slot % REF_COLORS.len()]));
        }
        out
    }
}

/// Rows the formula bar should show: one per line, one per ~90 characters.
fn bar_rows(text: &str) -> usize {
    let lines = text.lines().count().max(1);
    let wrapped = text.chars().count() / 90 + 1;
    lines.max(wrapped).clamp(1, 4)
}

/// The function name being typed just before the caret, if any.
fn partial_token(text: &str, caret: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let mut start = caret;
    while start > 0 && is_name_char(chars[start - 1]) {
        start -= 1;
    }
    if start == caret || !chars[start].is_ascii_alphabetic() {
        return None;
    }
    Some(chars[start..caret].iter().collect())
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_'
}

/// Cached suggestions narrowed down to what is being typed at the caret.
fn visible_completions(edit: &EditState, caret: usize) -> Vec<String> {
    if !edit.is_formula() || edit.completions.is_empty() {
        return Vec::new();
    }
    let Some(prefix) = partial_token(&edit.text, caret) else {
        return Vec::new();
    };
    let prefix = prefix.to_ascii_uppercase();
    edit.completions
        .iter()
        .filter(|name| name.to_ascii_uppercase().starts_with(&prefix))
        .cloned()
        .collect()
}

/// Replaces the partial name at the caret with `name(`, returning the new text
/// and caret position.
fn accept_completion(text: &str, caret: usize, name: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let mut start = caret;
    while start > 0 && is_name_char(chars[start - 1]) {
        start -= 1;
    }
    let mut out: String = chars[..start].iter().collect();
    out.push_str(name);
    out.push('(');
    let new_caret = start + name.chars().count() + 1;
    out.extend(chars[caret..].iter());
    (out, new_caret)
}

/// The char range a clicked reference should replace, or None if a reference
/// makes no sense at the caret.
fn reference_slot(text: &str, caret: usize) -> Option<(usize, usize)> {
    if !text.starts_with('=') {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    if caret == 0 {
        return None;
    }
    if REF_OPERATORS.contains(chars[caret - 1]) {
        return Some((caret, caret));
    }
    // At the end of the formula a trailing reference is replaced, the way
    // point mode does it.
    if caret == chars.len() {
        let mut start = caret;
        while start > 0 && (chars[start - 1].is_ascii_alphanumeric() || chars[start - 1] == '$') {
            start -= 1;
        }
        let token: String = chars[start..caret].iter().collect();
        if start > 0 && start < caret && parse_cell_ref(&token).is_some() {
            return Some((start, caret));
        }
    }
    None
}

/// Finds A1-style references in a formula: `$?[A-Z]{1,3}$?[0-9]+`, optionally
/// followed by `:` and a second such token. Function names (`LOG10(`), string
/// literals and other sheets' references are skipped.
fn scan_references(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            i += 1;
            continue;
        }
        let Some(end) = ref_token_end(&chars, i) else {
            i += 1;
            continue;
        };
        let after_name = i > 0 && (is_name_char(chars[i - 1]) || chars[i - 1] == '!');
        let is_call = chars.get(end) == Some(&'(');
        let name_continues = chars.get(end).is_some_and(|c| is_name_char(*c));
        if after_name || is_call || name_continues {
            i = end;
            continue;
        }
        let mut token_end = end;
        if chars.get(end) == Some(&':') {
            if let Some(second) = ref_token_end(&chars, end + 1) {
                token_end = second;
            }
        }
        out.push(chars[i..token_end].iter().collect());
        i = token_end;
    }
    out
}

/// End index of a `$?A$?1` token starting at `start`, if there is one.
fn ref_token_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    if chars.get(i) == Some(&'$') {
        i += 1;
    }
    let letters = i;
    while i < chars.len() && chars[i].is_ascii_alphabetic() && i - letters < 3 {
        i += 1;
    }
    if i == letters || chars.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    if chars.get(i) == Some(&'$') {
        i += 1;
    }
    let digits = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    Some(i)
}

/// Turns "A1" or "$A$1:B3" into a Range.
fn parse_reference(token: &str) -> Option<Range> {
    let (a, b) = token.split_once(':').unwrap_or((token, token));
    let (r0, c0) = parse_cell_ref(a)?;
    let (r1, c1) = parse_cell_ref(b)?;
    Some(Range {
        r0: r0.min(r1),
        r1: r0.max(r1),
        c0: c0.min(c1),
        c1: c0.max(c1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_references() {
        assert_eq!(scan_references("=SUM(A1:B3)+C4"), ["A1:B3", "C4"]);
        assert_eq!(scan_references("$A$1"), ["$A$1"]);
        assert!(scan_references("=CONCAT(\"hello\",\" world\")").is_empty());
        assert!(scan_references("plain text").is_empty());
    }

    #[test]
    fn skips_function_names_that_look_like_references() {
        assert_eq!(scan_references("=LOG10(A2)"), ["A2"]);
    }

    #[test]
    fn parses_scanned_references() {
        let r = parse_reference("A1:B3").unwrap();
        assert_eq!((r.r0, r.r1, r.c0, r.c1), (1, 3, 1, 2));
        let single = parse_reference("$C$7").unwrap();
        assert_eq!((single.r0, single.c0), (7, 3));
        assert!(parse_reference("nope").is_none());
    }

    #[test]
    fn reference_slots() {
        assert_eq!(reference_slot("=SUM(", 5), Some((5, 5)));
        assert_eq!(reference_slot("=1+", 3), Some((3, 3)));
        // Trailing reference is replaced, not appended to.
        assert_eq!(reference_slot("=A1", 3), Some((1, 3)));
        assert_eq!(reference_slot("=SUM(A1", 7), Some((5, 7)));
        assert_eq!(reference_slot("=hello", 6), None);
        assert_eq!(reference_slot("plain", 5), None);
    }

    #[test]
    fn accepts_completion_over_partial_name() {
        assert_eq!(
            accept_completion("=SU", 3, "SUM"),
            ("=SUM(".to_string(), 5)
        );
        assert_eq!(
            accept_completion("=1+av+2", 5, "AVERAGE"),
            ("=1+AVERAGE(+2".to_string(), 11)
        );
    }

    #[test]
    fn partial_tokens() {
        assert_eq!(partial_token("=SU", 3).as_deref(), Some("SU"));
        assert_eq!(partial_token("=SUM(", 5), None);
        assert_eq!(partial_token("=1+2", 4), None);
    }

    #[test]
    fn formula_bar_grows_with_content() {
        assert_eq!(bar_rows(""), 1);
        assert_eq!(bar_rows("=A1"), 1);
        assert_eq!(bar_rows("a\nb\nc"), 3);
        assert_eq!(bar_rows(&"x".repeat(200)), 3);
        assert_eq!(bar_rows(&"x".repeat(5000)), 4);
    }
}
