//! The ribbon and the full-screen File (backstage) view.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Context, Response, Sense, Ui, Vec2};

use enclave_ui::palette::{hex, palette_rows, swatch};
use enclave_ui::ribbon::{
    button_rect, color_button, icon_band, menu_face, paint_button_bg, paint_icon, paint_label,
    ribbon_group_ui,
};

use crate::commands::Command;
use crate::engine::StyleEdit;
use crate::state::{Dialog, Pending, RibbonTab, GridoApp};
use crate::ui::icons;

impl GridoApp {
    /// Ribbon button: large icon on top, label below, both centered.
    pub fn ribbon_button(
        &self,
        ui: &mut Ui,
        icon: &str,
        label: &str,
        cmd: Command,
        active: bool,
        out: &mut Vec<Command>,
    ) {
        let (rect, resp) = button_rect(ui, label);
        paint_button_bg(ui, rect, &resp, active);
        let color = ui.style().interact(&resp).text_color();
        paint_icon(
            ui.painter(),
            icon_band(rect, 0.0),
            icon,
            color,
            20.0,
            icons::draw,
        );
        paint_label(ui.painter(), rect, label, color);
        let resp = match self.keymap.shortcut_for(cmd) {
            Some(sc) => resp.on_hover_text(format!(
                "{}  ({})",
                cmd.label(),
                ui.ctx().format_shortcut(&sc)
            )),
            None => resp.on_hover_text(cmd.label()),
        };
        if resp.clicked() {
            out.push(cmd);
        }
    }

    /// A cluster of ribbon buttons with the group caption centered underneath.
    pub fn ribbon_group(
        &self,
        ui: &mut Ui,
        caption: &str,
        items: &[(&str, &str, Command)],
        out: &mut Vec<Command>,
    ) {
        ribbon_group_ui(ui, caption, |ui| {
            for (icon, label, cmd) in items {
                let active = self.command_is_active(*cmd);
                self.ribbon_button(ui, icon, label, *cmd, active, out);
            }
        });
    }

    /// Toggle buttons light up when the active cell (or sheet) is already in
    /// that state.
    fn command_is_active(&self, cmd: Command) -> bool {
        let fmt = self.engine.format(self.sheet, self.cursor.0, self.cursor.1);
        let frozen = self.engine.frozen(self.sheet);
        use crate::engine::HAlign;
        match cmd {
            Command::ToggleBold => fmt.bold,
            Command::ToggleItalic => fmt.italic,
            Command::ToggleUnderline => fmt.underline,
            Command::ToggleStrike => fmt.strike,
            Command::ToggleWrap => fmt.wrap,
            Command::AlignLeft => fmt.align == HAlign::Left,
            Command::AlignCenter => fmt.align == HAlign::Center,
            Command::AlignRight => fmt.align == HAlign::Right,
            Command::ToggleFilter => self.filter.is_some(),
            Command::FreezeTopRow => frozen == (1, 0),
            Command::FreezeFirstColumn => frozen == (0, 1),
            // Only "custom" freezes light up Freeze Panes; the top-row and
            // first-column cases have their own buttons.
            Command::FreezePanes => {
                (frozen.0 > 0 || frozen.1 > 0) && frozen != (1, 0) && frozen != (0, 1)
            }
            Command::ToggleGridLines => self.engine.show_grid_lines(self.sheet),
            _ => false,
        }
    }

    /// A ribbon dropdown listing commands. Keeps the tab compact:
    /// related actions live one click down instead of each taking a button.
    fn ribbon_menu(
        &self,
        ui: &mut Ui,
        icon: &str,
        label: &str,
        items: &[(&str, Command)],
        out: &mut Vec<Command>,
    ) {
        let resp = menu_face(ui, icon, label, icons::draw);
        egui::Popup::menu(&resp).show(|ui| {
            for (text, cmd) in items {
                if text.is_empty() {
                    ui.separator();
                    continue;
                }
                let mut button = egui::Button::new(*text);
                if let Some(sc) = self.keymap.shortcut_for(*cmd) {
                    button = button.shortcut_text(ui.ctx().format_shortcut(&sc));
                }
                if ui.add(button).clicked() {
                    out.push(*cmd);
                    ui.close();
                }
            }
        });
    }

    /// Shows the shared color palette anchored under `resp`. Returns the pick,
    /// where the inner `None` means the clear entry ("Automatic" / "No fill").
    fn palette_popup(&self, resp: &Response, clear_label: &str) -> Option<Option<[u8; 3]>> {
        let mut picked = None;
        egui::Popup::menu(resp).show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            if ui.button(clear_label).clicked() {
                picked = Some(None);
                ui.close();
            }
            ui.separator();
            for row in palette_rows() {
                ui.horizontal(|ui| {
                    for c in row {
                        if swatch(ui, c).clicked() {
                            picked = Some(Some(c));
                            ui.close();
                        }
                    }
                });
            }
            if !self.recent_colors.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Recent").weak().small());
                ui.horizontal(|ui| {
                    for c in &self.recent_colors {
                        if swatch(ui, *c).clicked() {
                            picked = Some(Some(*c));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            // The picker edits a draft color, kept in egui memory across frames.
            let id = ui.id().with("more-colors");
            let mut custom = ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, [0u8, 0, 0]));
            ui.horizontal(|ui| {
                if ui.color_edit_button_srgb(&mut custom).changed() {
                    picked = Some(Some(custom));
                }
                ui.label("More colors…");
            });
            ui.ctx().data_mut(|d| d.insert_temp(id, custom));
        });
        picked
    }

    /// Most recently used colors stay at the front, newest first, capped at 10.
    fn push_recent_color(&mut self, c: [u8; 3]) {
        self.recent_colors.retain(|x| *x != c);
        self.recent_colors.insert(0, c);
        self.recent_colors.truncate(10);
    }

    /// Applies a palette pick to the selection as fill or font color.
    fn apply_color(&mut self, fill: bool, choice: Option<[u8; 3]>) {
        let value = choice.map(hex);
        let edit = if fill {
            StyleEdit::FillColor(value)
        } else {
            StyleEdit::FontColor(value)
        };
        let (sheet, sel) = (self.sheet, self.selection());
        let result = self.engine.set_style(sheet, sel, edit);
        self.report("Color", result);
        if let Some(c) = choice {
            self.push_recent_color(c);
        }
        self.invalidate_geometry();
    }

    /// Save/Undo/Redo as a normal ribbon group, so they look and behave like
    /// every other button instead of hiding in the tab row.
    fn file_group(&self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        ribbon_group_ui(ui, "File", |ui| {
            self.ribbon_button(ui, "save", "Save", Command::Save, false, cmds);
            let can_undo = self.engine.can_undo();
            let can_redo = self.engine.can_redo();
            ui.add_enabled_ui(can_undo, |ui| {
                self.ribbon_button(ui, "undo", "Undo", Command::Undo, false, cmds);
            });
            ui.add_enabled_ui(can_redo, |ui| {
                self.ribbon_button(ui, "redo", "Redo", Command::Redo, false, cmds);
            });
        });
    }

    /// Backstage "Assistant" section: whether the MCP bridge is live, and the
    /// one-click way to make Claude (or another client) aware of Enclave.
    ///
    /// This exists so that connecting an assistant never requires a terminal.
    fn assistant_panel(&mut self, ui: &mut Ui) {
        // The socket belongs to the host; this window only probes the path it
        // was given, to tell "someone else is serving" from "nobody is".
        let listening = self
            .mcp_socket
            .as_ref()
            .is_some_and(|p| std::os::unix::net::UnixStream::connect(p).is_ok());
        let (dot, text) = match (self.mcp_serving, listening) {
            (true, _) => (
                Color32::from_rgb(60, 190, 100),
                "Ready — this window is the one an assistant edits".to_string(),
            ),
            (false, true) => (
                Color32::from_rgb(230, 180, 60),
                "Another Grido window is handling assistant requests".to_string(),
            ),
            (false, false) => (
                Color32::from_rgb(220, 90, 90),
                "Not available — could not open the connection".to_string(),
            ),
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, dot);
            ui.label(text);
        });
        ui.add_space(6.0);

        for client in crate::mcp::register::known_clients() {
            ui.horizontal(|ui| {
                let status = if !client.path.exists() {
                    "not installed".to_string()
                } else if client.registered {
                    "connected".to_string()
                } else {
                    "not connected".to_string()
                };
                ui.label(format!("{} — {status}", client.name));
                if client.path.exists() && !client.registered && ui.button("Connect").clicked() {
                    match crate::mcp::register::register_one(&client.path) {
                        Ok(_) => self.set_status(format!(
                            "Connected to {}. Restart it to pick up Enclave.",
                            client.name
                        )),
                        Err(e) => self.set_status(format!("Could not connect: {e}")),
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Connecting writes an \"enclave\" entry into the client's config \
                 (the previous file is kept as .bak). Restart the client afterwards.",
            )
            .weak()
            .small(),
        );

        if !self.activity.is_empty() {
            ui.add_space(14.0);
            ui.heading("What the assistant did");
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for entry in &self.activity {
                        let colour = if entry.ok {
                            ui.visuals().text_color()
                        } else {
                            Color32::from_rgb(220, 120, 120)
                        };
                        ui.label(egui::RichText::new(&entry.text).color(colour).small());
                    }
                });
        }
    }

    pub fn ribbon(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        egui::TopBottomPanel::top("ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(false, "File").clicked() {
                    self.backstage = true;
                }
                for (tab, label) in [
                    (RibbonTab::Home, "Home"),
                    (RibbonTab::Insert, "Insert"),
                    (RibbonTab::Data, "Data"),
                    (RibbonTab::View, "View"),
                    (RibbonTab::Sheet, "Sheet"),
                    (RibbonTab::Help, "Help"),
                ] {
                    if ui.selectable_label(self.ribbon_tab == tab, label).clicked() {
                        self.ribbon_tab = tab;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| match self.ribbon_tab {
                        RibbonTab::Home => self.tab_home(ui, &mut cmds),
                        RibbonTab::Insert => self.tab_insert(ui, &mut cmds),
                        RibbonTab::Data => self.tab_data(ui, &mut cmds),
                        RibbonTab::View => self.tab_view(ui, &mut cmds),
                        RibbonTab::Sheet => self.tab_sheet(ui, &mut cmds),
                        RibbonTab::Help => self.tab_help(ui),
                    });
                });
            ui.add_space(2.0);
        });
        cmds
    }

    fn tab_home(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        self.file_group(ui, cmds);
        self.ribbon_group(
            ui,
            "Clipboard",
            &[
                ("cut", "Cut", Command::Cut),
                ("copy", "Copy", Command::Copy),
                ("paste", "Paste", Command::Paste),
            ],
            cmds,
        );
        self.font_group(ui, cmds);
        self.ribbon_group(
            ui,
            "Alignment",
            &[
                ("align-left", "Left", Command::AlignLeft),
                ("align-center", "Center", Command::AlignCenter),
                ("align-right", "Right", Command::AlignRight),
            ],
            cmds,
        );
        ribbon_group_ui(ui, "Number", |ui| {
            self.ribbon_menu(
                ui,
                "cond-fmt",
                "Format",
                &[
                    ("General", Command::FormatGeneral),
                    ("Number", Command::FormatNumber),
                    ("Percent", Command::FormatPercent),
                    ("Comma", Command::FormatComma),
                    ("Date", Command::FormatDate),
                    ("", Command::FormatDialog),
                    ("Increase decimals", Command::IncreaseDecimal),
                    ("Decrease decimals", Command::DecreaseDecimal),
                    ("", Command::FormatDialog),
                    ("More number formats…", Command::FormatDialog),
                ],
                cmds,
            );
            self.ribbon_button(ui, "$", "Currency", Command::FormatCurrency, false, cmds);
        });
        ribbon_group_ui(ui, "Styles", |ui| {
            self.ribbon_button(
                ui,
                "▤",
                "Cond. Fmt",
                Command::ConditionalFormat,
                false,
                cmds,
            );
        });
        ribbon_group_ui(ui, "Cells", |ui| {
            self.ribbon_menu(
                ui,
                "insert",
                "Insert",
                &[
                    ("Insert row", Command::InsertRow),
                    ("Insert column", Command::InsertColumn),
                ],
                cmds,
            );
            self.ribbon_menu(
                ui,
                "delete",
                "Delete",
                &[
                    ("Delete row", Command::DeleteRow),
                    ("Delete column", Command::DeleteColumn),
                ],
                cmds,
            );
            self.ribbon_menu(
                ui,
                "resize",
                "Format",
                &[
                    ("Autofit column width", Command::AutofitColumn),
                    ("", Command::FreezePanes),
                    ("Freeze panes", Command::FreezePanes),
                    ("Unfreeze panes", Command::Unfreeze),
                ],
                cmds,
            );
        });
        ribbon_group_ui(ui, "Editing", |ui| {
            self.ribbon_menu(
                ui,
                "fill",
                "Fill",
                &[
                    ("Fill down", Command::FillDown),
                    ("Fill right", Command::FillRight),
                ],
                cmds,
            );
            self.ribbon_menu(
                ui,
                "clear",
                "Clear",
                &[
                    ("Clear contents", Command::Clear),
                    ("Clear formatting", Command::ClearFormats),
                    ("Clear all", Command::ClearAll),
                ],
                cmds,
            );
            self.ribbon_menu(
                ui,
                "sort",
                "Sort",
                &[
                    ("Sort A → Z", Command::SortAscending),
                    ("Sort Z → A", Command::SortDescending),
                    ("", Command::ToggleFilter),
                    ("Toggle filter", Command::ToggleFilter),
                ],
                cmds,
            );
            self.ribbon_button(ui, "🔍", "Find", Command::FindReplace, false, cmds);
        });
    }

    /// Font group: the three core toggles, the two color buttons, and a
    /// dropdown for the rest so the tab stays readable.
    fn font_group(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        ribbon_group_ui(ui, "Font", |ui| {
            for (icon, label, cmd) in [
                ("B", "Bold", Command::ToggleBold),
                ("I", "Italic", Command::ToggleItalic),
                ("U", "Under", Command::ToggleUnderline),
            ] {
                let active = self.command_is_active(cmd);
                self.ribbon_button(ui, icon, label, cmd, active, cmds);
            }
            self.ribbon_menu(
                ui,
                "Aa",
                "More",
                &[
                    ("Strikethrough", Command::ToggleStrike),
                    ("", Command::IncreaseFont),
                    ("Increase font size", Command::IncreaseFont),
                    ("Decrease font size", Command::DecreaseFont),
                    ("", Command::ToggleWrap),
                    ("Wrap text", Command::ToggleWrap),
                    ("", Command::FormatDialog),
                    ("Borders…", Command::BorderAll),
                    ("Format cells…", Command::FormatDialog),
                ],
                cmds,
            );
            let fmt = self.engine.format(self.sheet, self.cursor.0, self.cursor.1);
            let resp = color_button(ui, "A", "Font", fmt.font_color, icons::draw)
                .on_hover_text("Font color");
            let choice = self.palette_popup(&resp, "Automatic");
            if let Some(choice) = choice {
                self.apply_color(false, choice);
            }
            let resp = color_button(ui, "fill-color", "Fill", fmt.fill_color, icons::draw)
                .on_hover_text("Fill color");
            let choice = self.palette_popup(&resp, "No fill");
            if let Some(choice) = choice {
                self.apply_color(true, choice);
            }
        });
    }

    fn tab_insert(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        self.ribbon_group(
            ui,
            "Cells",
            &[
                ("insert", "Row", Command::InsertRow),
                ("insert", "Column", Command::InsertColumn),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Sheets",
            &[("sheet-new", "Sheet", Command::NewSheet)],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Charts",
            &[("chart", "Chart", Command::InsertChart)],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Names",
            &[("name", "Names", Command::NameManager)],
            cmds,
        );
    }

    fn tab_data(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        self.ribbon_group(
            ui,
            "Sort",
            &[
                ("sort-asc", "A-Z", Command::SortAscending),
                ("sort-desc", "Z-A", Command::SortDescending),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Filter",
            &[("filter", "Filter", Command::ToggleFilter)],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Find",
            &[
                ("find", "Find", Command::FindReplace),
                ("next", "Next", Command::FindNext),
                ("prev", "Prev", Command::FindPrev),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Names",
            &[("name", "Manager", Command::NameManager)],
            cmds,
        );
    }

    fn tab_view(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        self.ribbon_group(
            ui,
            "Freeze",
            &[
                ("freeze", "Panes", Command::FreezePanes),
                ("freeze-top", "Top Row", Command::FreezeTopRow),
                ("freeze-first", "First Col", Command::FreezeFirstColumn),
                ("unfreeze", "Unfreeze", Command::Unfreeze),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Zoom",
            &[
                ("zoom-in", "In", Command::ZoomIn),
                ("zoom-out", "Out", Command::ZoomOut),
                ("zoom-reset", "100%", Command::ZoomReset),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Show",
            &[("gridlines", "Gridlines", Command::ToggleGridLines)],
            cmds,
        );
    }

    fn tab_sheet(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        self.ribbon_group(
            ui,
            "Sheets",
            &[
                ("sheet-new", "New", Command::NewSheet),
                ("duplicate", "Duplicate", Command::DuplicateSheet),
                ("rename", "Rename", Command::RenameSheet),
                ("trash", "Delete", Command::DeleteSheet),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Order",
            &[
                ("prev", "Left", Command::MoveSheetLeft),
                ("next", "Right", Command::MoveSheetRight),
            ],
            cmds,
        );
        self.ribbon_group(
            ui,
            "Navigate",
            &[
                ("prev", "Previous", Command::PrevSheet),
                ("next", "Next", Command::NextSheet),
            ],
            cmds,
        );
        ribbon_group_ui(ui, "Tab color", |ui| {
            let current = self.engine.sheet_color(self.sheet);
            let resp = color_button(ui, "fill-color", "Color", current, icons::draw)
                .on_hover_text("Sheet tab color");
            let choice = self.palette_popup(&resp, "No color");
            if let Some(choice) = choice {
                let sheet = self.sheet;
                // An empty string clears the tab color in the engine.
                let value = choice.map(hex).unwrap_or_default();
                let result = self.engine.set_sheet_color(sheet, &value);
                self.report("Tab color", result);
                if let Some(c) = choice {
                    self.push_recent_color(c);
                }
            }
        });
    }

    fn tab_help(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label(format!("Keymap: {}", self.keymap.source));
            ui.label(
                egui::RichText::new(
                    "Edit keymap.toml to customize shortcuts (standard defaults built in).",
                )
                .weak()
                .small(),
            );
            if ui.button("Show all shortcuts").clicked() {
                self.dialog = Dialog::Shortcuts;
            }
        });
    }

    /// Full-screen File view: nav sidebar plus workbook info and recent files.
    pub fn backstage_ui(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.backstage = false;
            return cmds;
        }
        let mut open_recent: Option<PathBuf> = None;

        egui::SidePanel::left("backstage-nav")
            .resizable(false)
            .exact_width(190.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                if ui
                    .add(egui::Button::new(egui::RichText::new("←  Back").size(16.0)).frame(false))
                    .clicked()
                {
                    self.backstage = false;
                }
                ui.add_space(16.0);
                for (label, cmd) in [
                    ("🗋  New", Command::New),
                    ("🗁  Open…", Command::Open),
                    ("💾  Save", Command::Save),
                    ("🗐  Save As…", Command::SaveAs),
                ] {
                    let mut resp = ui.add_sized(
                        [ui.available_width(), 34.0],
                        egui::Button::new(egui::RichText::new(label).size(14.0)),
                    );
                    if let Some(sc) = self.keymap.shortcut_for(cmd) {
                        resp = resp.on_hover_text(ui.ctx().format_shortcut(&sc));
                    }
                    if resp.clicked() {
                        cmds.push(cmd);
                        self.backstage = false;
                    }
                    ui.add_space(4.0);
                }
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);
                if ui
                    .add_sized(
                        [ui.available_width(), 34.0],
                        egui::Button::new(egui::RichText::new("✖  Quit").size(14.0)),
                    )
                    .clicked()
                {
                    cmds.push(Command::Quit);
                    self.backstage = false;
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("Info");
            ui.add_space(4.0);
            match &self.engine.path {
                Some(p) => {
                    ui.label(
                        egui::RichText::new(
                            p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                        )
                        .strong(),
                    );
                    ui.label(egui::RichText::new(p.display().to_string()).weak());
                }
                None => {
                    ui.label(egui::RichText::new("Book1").strong());
                    ui.label(egui::RichText::new("Not saved yet").weak());
                }
            }
            let names = self.engine.sheet_names();
            ui.label(format!(
                "{} sheet(s): {}{}",
                names.len(),
                names.join(", "),
                if self.engine.dirty {
                    " — unsaved changes"
                } else {
                    ""
                }
            ));
            ui.add_space(16.0);
            ui.heading("Recent");
            ui.add_space(4.0);
            if self.recent.is_empty() {
                ui.label(egui::RichText::new("No recent files.").weak());
            }
            for path in &self.recent {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                if ui
                    .link(name)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    open_recent = Some(path.clone());
                }
            }

            ui.add_space(18.0);
            ui.heading("Assistant");
            ui.add_space(4.0);
            self.assistant_panel(ui);
        });

        if let Some(path) = open_recent {
            self.backstage = false;
            if self.engine.dirty {
                self.pending = Some(Pending::OpenPath(path));
            } else {
                self.open_path(path);
            }
        }
        cmds
    }
}
