//! Modal dialogs, the AutoFilter popup, sheet tabs and the status bar.

use eframe::egui::{self, Align2, Context, Key};

use crate::commands::Command;
use crate::engine::{col_letters, parse_cell_ref, BorderTarget, HAlign, StyleEdit, VAlign};
use crate::state::{Dialog, Pending, GridoApp};

/// Draft state for the AutoFilter dropdown. Kept in egui's temp memory so it
/// lives only as long as the popup is open.
#[derive(Clone, Default)]
struct FilterDraft {
    col: i32,
    search: String,
    /// (display value, checked)
    items: Vec<(String, bool)>,
}

/// Draft state for the Format cells dialog, reloaded from the active cell
/// whenever that cell changes.
#[derive(Clone)]
struct FormatDraft {
    tab: usize,
    /// (sheet, row, col) the controls were loaded from.
    key: (u32, i32, i32),
    /// 0 General, 1 Number, 2 Currency, 3 Percent, 4 Comma, 5 Date, 6 Custom
    num_kind: usize,
    decimals: i32,
    custom: String,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size: i32,
    font_color: [u8; 3],
    fill: [u8; 3],
    border_color: [u8; 3],
    align: HAlign,
    valign: VAlign,
    wrap: bool,
}

/// Extra sort levels; level 1 lives in `GridoApp::sort`.
#[derive(Clone, Copy)]
struct SortLevels {
    on2: bool,
    col2: i32,
    asc2: bool,
    on3: bool,
    col3: i32,
    asc3: bool,
}

impl Default for SortLevels {
    fn default() -> Self {
        SortLevels {
            on2: false,
            col2: 1,
            asc2: true,
            on3: false,
            col3: 1,
            asc3: true,
        }
    }
}

fn hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// Builds a number-format code from the dialog's controls.
fn num_fmt_code(kind: usize, decimals: i32, custom: &str) -> String {
    let with_decimals = |base: &str| {
        if decimals <= 0 {
            base.to_string()
        } else {
            format!("{base}.{}", "0".repeat(decimals as usize))
        }
    };
    match kind {
        1 => with_decimals("0"),
        2 => format!("\"$\"{}", with_decimals("#,##0")),
        3 => format!("{}%", with_decimals("0")),
        4 => with_decimals("#,##0"),
        5 => "yyyy-mm-dd".to_string(),
        6 => custom.to_string(),
        _ => "general".to_string(),
    }
}

/// Best-effort reverse of `num_fmt_code`, for showing the cell's current format.
fn detect_num_fmt(code: &str) -> (usize, i32) {
    let lower = code.to_lowercase();
    let decimals = lower
        .split_once('.')
        .map(|(_, rest)| rest.chars().take_while(|c| *c == '0').count() as i32)
        .unwrap_or(0);
    let kind = if lower.is_empty() || lower == "general" {
        0
    } else if lower.contains('%') {
        3
    } else if lower.contains('$') {
        2
    } else if lower.contains('y') || lower.contains("dd") || lower.contains("mmm") {
        5
    } else if lower.contains("#,##") {
        4
    } else if lower.chars().all(|c| "0#.,".contains(c)) {
        1
    } else {
        6
    };
    (kind, decimals)
}

/// A1-letter column chooser.
fn column_picker(ui: &mut egui::Ui, id: &str, col: &mut i32, max_col: i32) {
    egui::ComboBox::from_id_salt(id)
        .width(64.0)
        .selected_text(col_letters(*col))
        .show_ui(ui, |ui| {
            for c in 1..=max_col {
                ui.selectable_value(col, c, col_letters(c));
            }
        });
}

impl GridoApp {
    pub fn dialogs(&mut self, ctx: &Context) -> Vec<Command> {
        // The filter dropdown is not a Dialog: it can be open while the grid
        // still has focus.
        self.filter_popup(ctx);
        match self.dialog.clone() {
            Dialog::None => {}
            Dialog::FindReplace => self.find_replace_dialog(ctx),
            Dialog::GotoCell => self.goto_dialog(ctx),
            Dialog::Shortcuts => self.shortcuts_dialog(ctx),
            Dialog::RenameSheet { sheet, name } => self.rename_dialog(ctx, sheet, name),
            Dialog::Sort => self.sort_dialog(ctx),
            Dialog::NameManager => self.names_dialog(ctx),
            Dialog::FormatCells => self.format_dialog(ctx),
            Dialog::ConditionalFormat => self.conditional_format_dialog(ctx),
            Dialog::Chart => self.chart_dialog(ctx),
        }
        self.chart_windows(ctx);
        Vec::new()
    }

    /// Placeholder implementations live in `ui::charts` and are replaced there.
    fn conditional_format_dialog(&mut self, ctx: &Context) {
        self.cf_dialog_impl(ctx);
    }

    fn chart_dialog(&mut self, ctx: &Context) {
        self.chart_dialog_impl(ctx);
    }

    // ----- AutoFilter dropdown -----

    fn filter_popup(&mut self, ctx: &Context) {
        let Some(filter) = self.filter.clone() else {
            return;
        };
        let Some(col) = filter.open_column else {
            return;
        };
        let id = egui::Id::new("filter-draft");
        let mut draft: FilterDraft = ctx.data_mut(|d| d.get_temp(id)).unwrap_or_default();
        if draft.col != col {
            let values =
                self.engine
                    .distinct_values(self.sheet, col, filter.header_row + 1, filter.r1);
            let allowed = filter.allowed.get(&col);
            draft = FilterDraft {
                col,
                search: String::new(),
                items: values
                    .into_iter()
                    .map(|v| {
                        // A missing or empty entry means "everything allowed".
                        let on = allowed.map(|a| a.is_empty() || a.contains(&v)).unwrap_or(true);
                        (v, on)
                    })
                    .collect(),
            };
        }

        let mut close = false;
        let mut apply = false;
        let mut clear = false;
        let mut open = true;
        egui::Window::new(format!("Filter — column {}", col_letters(col)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::LEFT_TOP, [160.0, 110.0])
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.search)
                        .hint_text("Search")
                        .desired_width(200.0),
                );
                let needle = draft.search.to_lowercase();
                let visible: Vec<usize> = draft
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, (v, _))| needle.is_empty() || v.to_lowercase().contains(&needle))
                    .map(|(i, _)| i)
                    .collect();
                let mut all = visible.iter().all(|i| draft.items[*i].1);
                if ui.checkbox(&mut all, "(Select All)").changed() {
                    for i in &visible {
                        draft.items[*i].1 = all;
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for i in &visible {
                            let (value, checked) = &mut draft.items[*i];
                            let label = if value.is_empty() {
                                "(Blanks)".to_string()
                            } else {
                                value.clone()
                            };
                            ui.checkbox(checked, label);
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                if ui.button("Clear filter from this column").clicked() {
                    clear = true;
                }
            });

        if apply || clear || close || !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            if let Some(f) = &mut self.filter {
                if apply {
                    let all = draft.items.iter().all(|(_, on)| *on);
                    let allowed: Vec<String> = if all {
                        Vec::new()
                    } else {
                        draft
                            .items
                            .iter()
                            .filter(|(_, on)| *on)
                            .map(|(v, _)| v.clone())
                            .collect()
                    };
                    f.allowed.insert(col, allowed);
                } else if clear {
                    f.allowed.remove(&col);
                }
                f.open_column = None;
            }
            ctx.data_mut(|d| d.remove::<FilterDraft>(id));
            if apply || clear {
                self.apply_filter();
            }
        } else {
            ctx.data_mut(|d| d.insert_temp(id, draft));
        }
    }

    fn find_replace_dialog(&mut self, ctx: &Context) {
        let mut open = true;
        egui::Window::new("Find & Replace")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_TOP, [0.0, 80.0])
            .show(ctx, |ui| {
                egui::Grid::new("find-grid").num_columns(2).show(ui, |ui| {
                    ui.label("Find:");
                    let resp = ui.text_edit_singleline(&mut self.find.needle);
                    if resp.changed() {
                        self.find.hits.clear();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.run_find();
                    }
                    ui.end_row();
                    ui.label("Replace:");
                    ui.text_edit_singleline(&mut self.find.replacement);
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.find.match_case, "Match case");
                    ui.checkbox(&mut self.find.whole_workbook, "All sheets");
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Find all").clicked() {
                        self.run_find();
                    }
                    if ui.button("Next").clicked() {
                        self.find_step(true);
                    }
                    if ui.button("Previous").clicked() {
                        self.find_step(false);
                    }
                    if ui.button("Replace all").clicked() {
                        let (needle, replacement, case, workbook) = (
                            self.find.needle.clone(),
                            self.find.replacement.clone(),
                            self.find.match_case,
                            self.find.whole_workbook,
                        );
                        let sheet = self.sheet;
                        let n = self
                            .engine
                            .replace_all(sheet, &needle, &replacement, case, workbook);
                        self.find.message = format!("Replaced {n}");
                        self.find.hits.clear();
                    }
                });
                if !self.find.message.is_empty() {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(&self.find.message).weak());
                }
            });
        if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    fn goto_dialog(&mut self, ctx: &Context) {
        let mut open = true;
        let mut go: Option<(i32, i32)> = None;
        egui::Window::new("Go To")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Reference (e.g. B12):");
                let resp = ui.text_edit_singleline(&mut self.goto_text);
                resp.request_focus();
                let submit =
                    resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) || ui.button("Go").clicked();
                if submit {
                    go = parse_cell_ref(&self.goto_text);
                }
            });
        if let Some((row, col)) = go {
            self.set_cursor(row, col);
            self.goto_text.clear();
            self.dialog = Dialog::None;
        } else if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    fn rename_dialog(&mut self, ctx: &Context, sheet: u32, mut name: String) {
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Rename sheet")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let resp = ui.text_edit_singleline(&mut name);
                resp.request_focus();
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
                if ui.button("Rename").clicked() {
                    apply = true;
                }
            });
        if apply {
            let result = self.engine.rename_sheet(sheet, &name);
            self.report("Rename", result);
            self.dialog = Dialog::None;
        } else if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        } else {
            self.dialog = Dialog::RenameSheet { sheet, name };
        }
    }

    fn sort_dialog(&mut self, ctx: &Context) {
        let id = egui::Id::new("sort-levels");
        let mut levels: SortLevels = ctx.data_mut(|d| d.get_temp(id)).unwrap_or_default();
        let (_, used_cols) = self.engine.used_range(self.sheet);
        let max_col = used_cols.clamp(3, 200);
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Sort")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("sort-grid").num_columns(3).show(ui, |ui| {
                    ui.label("Sort by");
                    column_picker(ui, "sort-col-1", &mut self.sort.key_column, max_col);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.sort.ascending, true, "Asc");
                        ui.radio_value(&mut self.sort.ascending, false, "Desc");
                    });
                    ui.end_row();

                    ui.checkbox(&mut levels.on2, "Then by");
                    ui.add_enabled_ui(levels.on2, |ui| {
                        column_picker(ui, "sort-col-2", &mut levels.col2, max_col);
                    });
                    ui.add_enabled_ui(levels.on2, |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut levels.asc2, true, "Asc");
                            ui.radio_value(&mut levels.asc2, false, "Desc");
                        });
                    });
                    ui.end_row();

                    ui.checkbox(&mut levels.on3, "Then by");
                    ui.add_enabled_ui(levels.on3, |ui| {
                        column_picker(ui, "sort-col-3", &mut levels.col3, max_col);
                    });
                    ui.add_enabled_ui(levels.on3, |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut levels.asc3, true, "Asc");
                            ui.radio_value(&mut levels.asc3, false, "Desc");
                        });
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.sort.has_header, "My data has headers");
                ui.add_space(6.0);
                if ui.button("Sort").clicked() {
                    apply = true;
                }
            });
        ctx.data_mut(|d| d.insert_temp(id, levels));
        if apply {
            let (ur, uc) = self.engine.used_range(self.sheet);
            let r0 = if self.sort.has_header { 2 } else { 1 };
            let range = crate::engine::Range {
                r0,
                r1: ur,
                c0: 1,
                c1: uc,
            };
            let mut keys = vec![(self.sort.key_column, self.sort.ascending)];
            if levels.on2 {
                keys.push((levels.col2, levels.asc2));
            }
            if levels.on3 {
                keys.push((levels.col3, levels.asc3));
            }
            // `sort_range` is a stable sort, so sorting the least significant
            // key first and the primary key last gives multi-key ordering.
            let sheet = self.sheet;
            for (key, asc) in keys.iter().rev() {
                let result = self.engine.sort_range(sheet, range, *key, *asc);
                self.report("Sort", result);
            }
            self.invalidate_geometry();
            self.dialog = Dialog::None;
        } else if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    fn names_dialog(&mut self, ctx: &Context) {
        let mut open = true;
        egui::Window::new("Name manager")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let names = self.engine.defined_names();
                if names.is_empty() {
                    ui.label(egui::RichText::new("No defined names yet.").weak());
                }
                let mut delete: Option<String> = None;
                for (name, formula) in &names {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).strong());
                        ui.label(egui::RichText::new(formula).weak());
                        if ui.small_button("🗑").clicked() {
                            delete = Some(name.clone());
                        }
                    });
                }
                if let Some(name) = delete {
                    let result = self.engine.delete_defined_name(&name);
                    self.report("Delete name", result);
                }
                ui.separator();
                ui.label("New name");
                egui::Grid::new("names-grid").num_columns(2).show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.names.new_name);
                    ui.end_row();
                    ui.label("Refers to:");
                    ui.text_edit_singleline(&mut self.names.new_formula);
                    ui.end_row();
                });
                if ui.button("Add").clicked() {
                    let (name, formula) =
                        (self.names.new_name.clone(), self.names.new_formula.clone());
                    match self.engine.add_defined_name(&name, &formula) {
                        Ok(()) => {
                            self.names.new_name.clear();
                            self.names.new_formula.clear();
                            self.names.message = format!("Added {name}");
                        }
                        Err(e) => self.names.message = format!("{e}"),
                    }
                }
                if !self.names.message.is_empty() {
                    ui.label(egui::RichText::new(&self.names.message).weak());
                }
            });
        if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    // ----- format cells -----

    /// Tabbed Format cells dialog. Every control applies to the selection as
    /// soon as it changes; only the custom format code waits for its button.
    fn format_dialog(&mut self, ctx: &Context) {
        let id = egui::Id::new("format-draft");
        let key = (self.sheet, self.cursor.0, self.cursor.1);
        let mut draft: Option<FormatDraft> = ctx.data_mut(|d| d.get_temp(id));
        if draft.as_ref().map(|d| d.key) != Some(key) {
            let fmt = self.engine.format(key.0, key.1, key.2);
            let (num_kind, decimals) = detect_num_fmt(&fmt.num_fmt);
            draft = Some(FormatDraft {
                tab: draft.map(|d| d.tab).unwrap_or(0),
                key,
                num_kind,
                decimals,
                custom: fmt.num_fmt.clone(),
                bold: fmt.bold,
                italic: fmt.italic,
                underline: fmt.underline,
                strike: fmt.strike,
                size: fmt.font_size as i32,
                font_color: fmt.font_color.unwrap_or([0, 0, 0]),
                fill: fmt.fill_color.unwrap_or([255, 255, 255]),
                border_color: [74, 74, 74],
                align: fmt.align,
                valign: fmt.valign,
                wrap: fmt.wrap,
            });
        }
        let mut draft = draft.expect("draft is set above");

        let mut open = true;
        egui::Window::new("Format cells")
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, label) in ["Number", "Font", "Alignment", "Border", "Fill"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut draft.tab, i, *label);
                    }
                });
                ui.separator();
                match draft.tab {
                    0 => self.format_number_tab(ui, &mut draft),
                    1 => self.format_font_tab(ui, &mut draft),
                    2 => self.format_align_tab(ui, &mut draft),
                    3 => self.format_border_tab(ui, &mut draft),
                    _ => self.format_fill_tab(ui, &mut draft),
                }
            });
        ctx.data_mut(|d| d.insert_temp(id, draft));
        if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            ctx.data_mut(|d| d.remove::<FormatDraft>(id));
            self.dialog = Dialog::None;
        }
    }

    fn format_number_tab(&mut self, ui: &mut egui::Ui, draft: &mut FormatDraft) {
        let mut changed = false;
        for (i, label) in [
            "General",
            "Number",
            "Currency",
            "Percent",
            "Comma",
            "Date",
            "Custom",
        ]
        .iter()
        .enumerate()
        {
            if ui.radio_value(&mut draft.num_kind, i, *label).changed() {
                changed = true;
            }
        }
        if (1..=4).contains(&draft.num_kind) {
            ui.horizontal(|ui| {
                ui.label("Decimal places:");
                if ui
                    .add(egui::DragValue::new(&mut draft.decimals).range(0..=10))
                    .changed()
                {
                    changed = true;
                }
            });
        }
        ui.add_space(4.0);
        ui.label("Format code:");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut draft.custom);
            if ui.button("Apply code").clicked() {
                draft.num_kind = 6;
                changed = true;
            }
        });
        if changed {
            let code = num_fmt_code(draft.num_kind, draft.decimals, &draft.custom);
            draft.custom = code.clone();
            self.style_selection(StyleEdit::NumFmt(code));
        }
    }

    fn format_font_tab(&mut self, ui: &mut egui::Ui, draft: &mut FormatDraft) {
        if ui.checkbox(&mut draft.bold, "Bold").changed() {
            self.style_selection(StyleEdit::Bold(draft.bold));
        }
        if ui.checkbox(&mut draft.italic, "Italic").changed() {
            self.style_selection(StyleEdit::Italic(draft.italic));
        }
        if ui.checkbox(&mut draft.underline, "Underline").changed() {
            self.style_selection(StyleEdit::Underline(draft.underline));
        }
        if ui.checkbox(&mut draft.strike, "Strikethrough").changed() {
            self.style_selection(StyleEdit::Strike(draft.strike));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Size:");
            if ui
                .add(egui::DragValue::new(&mut draft.size).range(6..=96))
                .changed()
            {
                self.style_selection(StyleEdit::FontSize(draft.size));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Color:");
            if ui.color_edit_button_srgb(&mut draft.font_color).changed() {
                self.style_selection(StyleEdit::FontColor(Some(hex(draft.font_color))));
            }
        });
    }

    fn format_align_tab(&mut self, ui: &mut egui::Ui, draft: &mut FormatDraft) {
        ui.label("Horizontal");
        ui.horizontal(|ui| {
            for (value, label) in [
                (HAlign::General, "General"),
                (HAlign::Left, "Left"),
                (HAlign::Center, "Center"),
                (HAlign::Right, "Right"),
            ] {
                if ui.radio_value(&mut draft.align, value, label).changed() {
                    self.style_selection(StyleEdit::Align(value));
                }
            }
        });
        ui.add_space(4.0);
        ui.label("Vertical");
        ui.horizontal(|ui| {
            for (value, label) in [
                (VAlign::Top, "Top"),
                (VAlign::Center, "Center"),
                (VAlign::Bottom, "Bottom"),
            ] {
                if ui.radio_value(&mut draft.valign, value, label).changed() {
                    self.style_selection(StyleEdit::VAlign(value));
                }
            }
        });
        ui.add_space(4.0);
        if ui.checkbox(&mut draft.wrap, "Wrap text").changed() {
            self.style_selection(StyleEdit::Wrap(draft.wrap));
        }
    }

    fn format_border_tab(&mut self, ui: &mut egui::Ui, draft: &mut FormatDraft) {
        ui.horizontal(|ui| {
            ui.label("Color:");
            ui.color_edit_button_srgb(&mut draft.border_color);
        });
        ui.add_space(4.0);
        let mut target = None;
        ui.horizontal_wrapped(|ui| {
            for (label, value) in [
                ("All", BorderTarget::All),
                ("Outer", BorderTarget::Outer),
                ("Bottom", BorderTarget::Bottom),
                ("Top", BorderTarget::Top),
                ("Left", BorderTarget::Left),
                ("Right", BorderTarget::Right),
                ("None", BorderTarget::None),
            ] {
                if ui.button(label).clicked() {
                    target = Some(value);
                }
            }
        });
        if let Some(target) = target {
            let (sheet, range) = (self.sheet, self.selection());
            let color = hex(draft.border_color);
            let result = self.engine.set_border(sheet, range, target, &color);
            self.report("Border", result);
            self.invalidate_geometry();
        }
    }

    fn format_fill_tab(&mut self, ui: &mut egui::Ui, draft: &mut FormatDraft) {
        ui.horizontal(|ui| {
            ui.label("Fill color:");
            if ui.color_edit_button_srgb(&mut draft.fill).changed() {
                self.style_selection(StyleEdit::FillColor(Some(hex(draft.fill))));
            }
        });
        ui.add_space(4.0);
        if ui.button("No fill").clicked() {
            self.style_selection(StyleEdit::FillColor(None));
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &Context) {
        let mut open = true;
        egui::Window::new("Keyboard shortcuts")
            .collapsible(false)
            .resizable(true)
            .default_height(500.0)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("Loaded from: {}", self.keymap.source)).weak(),
                );
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("shortcut-grid")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (shortcut, cmd) in &self.keymap.bindings {
                                ui.label(ui.ctx().format_shortcut(shortcut));
                                ui.label(cmd.label());
                                ui.end_row();
                            }
                        });
                });
            });
        if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    pub fn confirm_dialog(&mut self, ctx: &Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        let mut choice: Option<&str> = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("This workbook has unsaved changes.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some("save");
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some("discard");
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some("cancel");
                    }
                });
            });
        match choice {
            Some("save") => {
                self.pending = None;
                if self.do_save() {
                    self.run_pending(action, ctx);
                }
            }
            Some("discard") => {
                self.pending = None;
                // Discarding means the workbook is no longer "dirty" for the
                // purposes of the action we are about to run.
                self.engine.dirty = false;
                self.run_pending(action, ctx);
            }
            Some("cancel") => self.pending = None,
            _ => {}
        }
    }

    pub fn sheet_tabs(&mut self, ctx: &Context) {
        egui::TopBottomPanel::bottom("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let names = self.engine.sheet_names();
                let mut switch = None;
                let mut rename: Option<(u32, String)> = None;
                // (sheet to move, destination index)
                let mut move_to: Option<(u32, u32)> = None;
                for (i, name) in names.iter().enumerate() {
                    let idx = i as u32;
                    let resp = ui.selectable_label(idx == self.sheet, name);
                    if let Some(color) = self.engine.sheet_color(idx) {
                        let rect = resp.rect;
                        let underline = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom() - 3.0),
                            rect.right_bottom(),
                        );
                        ui.painter().rect_filled(
                            underline,
                            egui::CornerRadius::ZERO,
                            egui::Color32::from_rgb(color[0], color[1], color[2]),
                        );
                    }
                    if resp.clicked() {
                        switch = Some(idx);
                    }
                    if resp.double_clicked() {
                        rename = Some((idx, name.clone()));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Rename").clicked() {
                            rename = Some((idx, name.clone()));
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            let _ = self.engine.duplicate_sheet(idx);
                            ui.close();
                        }
                        if ui.button("Move left").clicked() {
                            if idx > 0 {
                                move_to = Some((idx, idx - 1));
                            }
                            ui.close();
                        }
                        if ui.button("Move right").clicked() {
                            if (idx as usize) + 1 < names.len() {
                                move_to = Some((idx, idx + 1));
                            }
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            if names.len() > 1 {
                                let _ = self.engine.delete_sheet(idx);
                                switch = Some(0);
                            }
                            ui.close();
                        }
                    });
                }
                if ui.button("+").on_hover_text("New sheet").clicked() {
                    if self.engine.new_sheet().is_ok() {
                        switch = Some(names.len() as u32);
                    }
                }
                if let Some((from, to)) = move_to {
                    // The context menu acts on the clicked tab, so follow it to
                    // its new position.
                    self.switch_sheet(from);
                    let result = self.engine.move_sheet(from, to);
                    self.report("Move sheet", result);
                    self.sheet = to;
                }
                if let Some(s) = switch {
                    self.sheet = self.sheet.min(
                        (self.engine.sheet_names().len() as u32).saturating_sub(1),
                    );
                    self.switch_sheet(s);
                }
                if let Some((sheet, name)) = rename {
                    self.dialog = Dialog::RenameSheet { sheet, name };
                }
            });
        });
    }

    /// Status bar: selection stats on the right, zoom level.
    pub fn status_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.status).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{}%", (self.zoom * 100.0).round() as i32));
                    ui.spacing_mut().slider_width = 90.0;
                    let slider = ui.add(
                        egui::Slider::new(&mut self.zoom, 0.4..=4.0)
                            .show_value(false)
                            .fixed_decimals(2),
                    );
                    if slider.changed() {
                        self.invalidate_geometry();
                    }
                    ui.separator();
                    let sel = self.selection();
                    if sel.rows() > 1 || sel.cols() > 1 {
                        let (count, numeric, sum, avg) = self.engine.stats(self.sheet, sel);
                        if numeric > 0 {
                            ui.label(format!("Sum: {sum:.2}"));
                            ui.separator();
                            ui.label(format!("Average: {avg:.2}"));
                            ui.separator();
                        }
                        ui.label(format!("Count: {count}"));
                        ui.separator();
                    }
                    let (rows, cols) = self.engine.frozen(self.sheet);
                    if rows > 0 || cols > 0 {
                        ui.label(format!("Frozen: {rows} row(s), {cols} col(s)"));
                        ui.separator();
                    }
                });
            });
        });
    }
}

/// Kept for the app module's Pending import to stay meaningful in one place.
pub type PendingAction = Pending;
