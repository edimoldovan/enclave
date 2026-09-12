//! The cell grid: geometry, painting and mouse interaction.

use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, ScrollArea,
    Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
};

use crate::commands::Command;
use crate::engine::{CellFormat, HAlign, Range, VAlign};
use crate::state::{
    EditMode, FillDrag, Resizing, GridoApp, BASE_FONT, CELL_PAD, GUTTER_W, HEADER_H, RESIZE_GRAB,
};
use enclave_ui::color::rgb;

/// Colors resolved once per frame and shared by the scrolled pass and the
/// frozen-pane overlay.
struct GridPaint {
    text: Color32,
    accent: Color32,
    line: Stroke,
    show_lines: bool,
    /// Opaque sheet background, used where an overlay must hide what is under it.
    bg: Color32,
}

/// The merged region containing a cell, if any.
fn merged_at(merges: &[Range], row: i32, col: i32) -> Option<Range> {
    merges.iter().copied().find(|m| m.contains(row, col))
}

fn overlaps(a: Range, r0: i32, r1: i32, c0: i32, c1: i32) -> bool {
    a.r1 >= r0 && a.r0 <= r1 && a.c1 >= c0 && a.c0 <= c1
}

impl GridoApp {
    // ----- geometry -----

    pub fn ensure_geometry(&mut self) {
        if !self.geom_dirty
            && self.row_off.len() == self.view_rows as usize + 1
            && self.col_off.len() == self.view_cols as usize + 1
        {
            return;
        }
        let zoom = self.zoom;
        self.row_off.clear();
        self.row_off.reserve(self.view_rows as usize + 1);
        self.row_off.push(0.0);
        let mut y = 0.0f32;
        for r in 1..=self.view_rows {
            y += self.engine.row_height(self.sheet, r) * zoom;
            self.row_off.push(y);
        }
        self.col_off.clear();
        self.col_off.reserve(self.view_cols as usize + 1);
        self.col_off.push(0.0);
        let mut x = 0.0f32;
        for c in 1..=self.view_cols {
            x += self.engine.col_width(self.sheet, c) * zoom;
            self.col_off.push(x);
        }
        self.geom_dirty = false;
    }

    pub fn cell_rect(&self, row: i32, col: i32, origin: Pos2) -> Rect {
        let r = (row as usize).min(self.row_off.len() - 1).max(1);
        let c = (col as usize).min(self.col_off.len() - 1).max(1);
        Rect::from_min_max(
            origin + Vec2::new(self.col_off[c - 1], self.row_off[r - 1]),
            origin + Vec2::new(self.col_off[c], self.row_off[r]),
        )
    }

    /// Bounding rect of a whole range.
    fn range_rect(&self, range: Range, origin: Pos2) -> Rect {
        Rect::from_min_max(
            self.cell_rect(range.r0, range.c0, origin).min,
            self.cell_rect(range.r1, range.c1, origin).max,
        )
    }

    pub fn cell_at(&self, pos: Pos2, origin: Pos2) -> (i32, i32) {
        let p = pos - origin;
        let row = self.row_off.partition_point(|&y| y <= p.y).max(1) as i32;
        let col = self.col_off.partition_point(|&x| x <= p.x).max(1) as i32;
        (row.min(self.view_rows), col.min(self.view_cols))
    }

    fn row_at_y(&self, y: f32, origin_y: f32) -> i32 {
        (self.row_off.partition_point(|&v| v <= y - origin_y).max(1) as i32).min(self.view_rows)
    }

    fn col_at_x(&self, x: f32, origin_x: f32) -> i32 {
        (self.col_off.partition_point(|&v| v <= x - origin_x).max(1) as i32).min(self.view_cols)
    }

    /// Visible row span for a viewport that starts `offset` px into the sheet.
    fn visible_rows(&self, offset: f32, height: f32) -> (i32, i32) {
        let r0 = self.row_off.partition_point(|&y| y <= offset).max(1) as i32;
        let r1 = (self.row_off.partition_point(|&y| y < offset + height) as i32)
            .clamp(r0, self.view_rows);
        (r0, r1)
    }

    fn visible_cols(&self, offset: f32, width: f32) -> (i32, i32) {
        let c0 = self.col_off.partition_point(|&x| x <= offset).max(1) as i32;
        let c1 =
            (self.col_off.partition_point(|&x| x < offset + width) as i32).clamp(c0, self.view_cols);
        (c0, c1)
    }

    fn paint_style(&self, ui: &Ui) -> GridPaint {
        let visuals = ui.visuals();
        GridPaint {
            text: visuals.text_color(),
            accent: visuals.selection.stroke.color,
            line: Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
            show_lines: self.engine.show_grid_lines(self.sheet),
            bg: visuals.panel_fill,
        }
    }

    // ----- top level -----

    pub fn grid(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        let bg = ctx.style().visuals.panel_fill;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg))
            .show(ctx, |ui| {
                let avail = ui.available_rect_before_wrap();
                let grid_rect = Rect::from_min_max(
                    Pos2::new(avail.min.x + GUTTER_W, avail.min.y + HEADER_H),
                    avail.max,
                );
                let row_h = (21.0 * self.zoom).max(4.0);
                self.rows_per_page = ((grid_rect.height() / row_h) as i32 - 1).max(1);

                let mut grid_ui = ui.new_child(UiBuilder::new().max_rect(grid_rect));
                grid_ui.set_clip_rect(grid_rect);
                let output = ScrollArea::both()
                    .auto_shrink([false, false])
                    .show_viewport(&mut grid_ui, |ui, viewport| {
                        self.grid_contents(ui, viewport, &mut cmds);
                    });
                let offset = output.state.offset;

                self.frozen_overlay(ui, grid_rect, offset);
                self.headers(ui, avail, grid_rect, offset, &mut cmds);
                self.edit_overlay(ctx, grid_rect, offset);
            });
        cmds
    }

    fn grid_contents(&mut self, ui: &mut Ui, viewport: Rect, cmds: &mut Vec<Command>) {
        self.ensure_geometry();
        let total = Vec2::new(
            self.col_off[self.view_cols as usize],
            self.row_off[self.view_rows as usize],
        );
        ui.set_min_size(total);
        let origin = ui.min_rect().min;

        let (r0, r1) = self.visible_rows(viewport.min.y, viewport.height());
        let (c0, c1) = self.visible_cols(viewport.min.x, viewport.width());
        let merges = self.engine.merges(self.sheet);

        // The fill handle sits on top of the grid, so clicks there must not
        // move the selection.
        let sel_rect = self.range_rect(self.selection(), origin);
        let handle_rect = Rect::from_center_size(sel_rect.max, Vec2::splat(8.0));

        let response = ui.interact(
            Rect::from_min_size(origin, total),
            Id::new("sheetz-grid"),
            Sense::click_and_drag(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        let on_handle = response
            .interact_pointer_pos()
            .is_some_and(|p| handle_rect.expand(2.0).contains(p));
        if !on_handle && self.fill_drag.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                let cell = self.cell_at(pos, origin);
                let merge = merged_at(&merges, cell.0, cell.1);
                if response.double_clicked() {
                    let (row, col) = merge.map_or(cell, |m| (m.r0, m.c0));
                    self.set_cursor(row, col);
                    let text = self.engine.content(self.sheet, row, col);
                    self.begin_edit(text, EditMode::Edit);
                } else if response.drag_started() || response.clicked() {
                    self.select_cell(cell, merge, shift);
                } else if response.dragged() {
                    self.cursor = merge.map_or(cell, |m| (m.r0, m.c0));
                }
            }
        }
        response.context_menu(|ui| {
            self.cell_context_menu(ui, cmds);
        });

        let st = self.paint_style(ui);
        let painter = ui.painter().clone();
        self.paint_block(&painter, origin, (r0, r1), (c0, c1), &st, &merges);

        // Preview of the range a fill-handle drag would cover.
        if let Some(fill) = self.fill_drag {
            let extended = Range {
                r0: fill.origin.r0,
                r1: fill.to_row.max(fill.origin.r1),
                c0: fill.origin.c0,
                c1: fill.to_col.max(fill.origin.c1),
            };
            painter.rect_stroke(
                self.range_rect(extended, origin),
                CornerRadius::ZERO,
                Stroke::new(1.5, st.accent),
                StrokeKind::Outside,
            );
        }

        // Ranges the assistant just wrote, tinted and fading out so the user
        // can see what changed without hunting for it.
        for change in &self.recent_changes {
            if change.sheet != self.sheet {
                continue;
            }
            let strength = (change.ttl / 6.0).clamp(0.0, 1.0);
            let rect = self.range_rect(change.range, origin);
            let a = st.accent;
            painter.rect_filled(
                rect,
                0.0,
                Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), (55.0 * strength) as u8),
            );
            painter.rect_stroke(
                rect,
                CornerRadius::ZERO,
                Stroke::new(
                    1.5,
                    Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), (200.0 * strength) as u8),
                ),
                StrokeKind::Outside,
            );
        }

        if let Some((sheet, range)) = self.cut_marker {
            if sheet == self.sheet {
                painter.rect_stroke(
                    self.range_rect(range, origin),
                    CornerRadius::ZERO,
                    Stroke::new(1.5, Color32::from_gray(150)),
                    StrokeKind::Outside,
                );
            }
        }

        self.fill_handle(ui, &painter, origin, handle_rect, &st);
        self.filter_buttons(ui, &painter, origin, (r0, r1), (c0, c1), &st);

        if self.scroll_to_cursor {
            let active = self.cell_rect(self.cursor.0, self.cursor.1, origin);
            ui.scroll_to_rect(active.expand(2.0), None);
            self.scroll_to_cursor = false;
        }
    }

    fn select_cell(&mut self, cell: (i32, i32), merge: Option<Range>, shift: bool) {
        match merge {
            Some(m) => {
                self.cursor = (m.r0, m.c0);
                if !shift {
                    self.anchor = (m.r1, m.c1);
                }
            }
            None => {
                self.cursor = cell;
                if !shift {
                    self.anchor = cell;
                }
            }
        }
    }

    // ----- painting -----

    /// Paints one rectangular block of cells: fills, selection wash, grid
    /// lines, merged regions, then values. Used by both the scrolled pass and
    /// the frozen-pane overlay, so `origin` may differ per axis from the real
    /// sheet origin.
    fn paint_block(
        &self,
        painter: &egui::Painter,
        origin: Pos2,
        rows: (i32, i32),
        cols: (i32, i32),
        st: &GridPaint,
        merges: &[Range],
    ) {
        let (r0, r1) = rows;
        let (c0, c1) = cols;
        if r1 < r0 || c1 < c0 {
            return;
        }
        let block = Rect::from_min_max(
            self.cell_rect(r0, c0, origin).min,
            self.cell_rect(r1, c1, origin).max,
        );

        // Cell fills first, so grid lines and text land on top.
        for r in r0..=r1 {
            for c in c0..=c1 {
                if merged_at(merges, r, c).is_some() {
                    continue;
                }
                if let Some(fill) = self.engine.format(self.sheet, r, c).fill_color {
                    painter.rect_filled(self.cell_rect(r, c, origin), 0.0, rgb(fill));
                }
            }
        }

        let sel = self.selection();
        let wash = st.accent.linear_multiply(0.15);
        if overlaps(sel, r0, r1, c0, c1) {
            painter.rect_filled(self.range_rect(sel, origin), 0.0, wash);
        }

        if st.show_lines {
            for r in (r0 - 1)..=r1 {
                let y = origin.y + self.row_off[r as usize];
                painter.hline(block.min.x..=block.max.x, y, st.line);
            }
            for c in (c0 - 1)..=c1 {
                let x = origin.x + self.col_off[c as usize];
                painter.vline(x, block.min.y..=block.max.y, st.line);
            }
        }

        // A merged region paints as one cell, hiding the interior grid lines.
        for m in merges.iter().copied() {
            if !overlaps(m, r0, r1, c0, c1) {
                continue;
            }
            let rect = self.range_rect(m, origin);
            let fill = self
                .engine
                .format(self.sheet, m.r0, m.c0)
                .fill_color
                .map(rgb)
                .unwrap_or(st.bg);
            painter.rect_filled(rect, 0.0, fill);
            if overlaps(sel, m.r0, m.r1, m.c0, m.c1) {
                painter.rect_filled(rect, 0.0, wash);
            }
            if st.show_lines {
                painter.rect_stroke(rect, CornerRadius::ZERO, st.line, StrokeKind::Inside);
            }
        }

        for r in r0..=r1 {
            for c in c0..=c1 {
                if merged_at(merges, r, c).is_some() {
                    continue;
                }
                self.paint_cell(painter, self.cell_rect(r, c, origin), r, c, st.text);
            }
        }
        for m in merges.iter().copied() {
            if overlaps(m, r0, r1, c0, c1) {
                self.paint_cell(painter, self.range_rect(m, origin), m.r0, m.c0, st.text);
            }
        }

        // Active cell outline.
        let (cr, cc) = self.cursor;
        if cr >= r0 && cr <= r1 && cc >= c0 && cc <= c1 {
            let rect = match merged_at(merges, cr, cc) {
                Some(m) => self.range_rect(m, origin),
                None => self.cell_rect(cr, cc, origin),
            };
            painter.rect_stroke(
                rect,
                CornerRadius::ZERO,
                Stroke::new(2.0, st.accent),
                StrokeKind::Inside,
            );
        }
    }

    /// Data bar, borders and text for one cell. `rect` is the whole merged
    /// region when the cell is the anchor of a merge.
    fn paint_cell(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        row: i32,
        col: i32,
        default_color: Color32,
    ) {
        // Hidden rows and zero-width columns have nothing to show.
        if rect.height() < 1.0 || rect.width() < 1.0 {
            return;
        }
        let fmt = self.engine.format(self.sheet, row, col);

        if let Some(bar) = fmt.data_bar {
            let inner = rect.shrink(2.0);
            let x0 = inner.min.x + inner.width() * bar.axis;
            let x1 = (x0 + inner.width() * bar.fill).min(inner.max.x);
            if x1 > x0 {
                let color =
                    Color32::from_rgba_unmultiplied(bar.color[0], bar.color[1], bar.color[2], 170);
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(x0, inner.min.y), Pos2::new(x1, inner.max.y)),
                    0.0,
                    color,
                );
            }
        }

        self.paint_borders(painter, rect, &fmt);

        let text = self.engine.display(self.sheet, row, col);
        if text.is_empty() {
            return;
        }
        let mut font = FontId::proportional(fmt.font_size * self.zoom * (BASE_FONT / 11.0));
        if font.size < 5.0 {
            font.size = 5.0;
        }
        let color = fmt.font_color.map(rgb).unwrap_or(default_color);
        let align = match fmt.align {
            HAlign::General => {
                if self.engine.is_number(self.sheet, row, col) {
                    HAlign::Right
                } else {
                    HAlign::Left
                }
            }
            other => other,
        };
        let y = match fmt.valign {
            VAlign::Top => rect.min.y + CELL_PAD,
            VAlign::Bottom => rect.max.y - CELL_PAD,
            VAlign::Center => rect.center().y,
        };
        let (pos, anchor) = match (align, fmt.valign) {
            (HAlign::Right, VAlign::Top) => (Pos2::new(rect.max.x - CELL_PAD, y), Align2::RIGHT_TOP),
            (HAlign::Right, VAlign::Bottom) => {
                (Pos2::new(rect.max.x - CELL_PAD, y), Align2::RIGHT_BOTTOM)
            }
            (HAlign::Right, _) => (Pos2::new(rect.max.x - CELL_PAD, y), Align2::RIGHT_CENTER),
            (HAlign::Center, VAlign::Top) => (Pos2::new(rect.center().x, y), Align2::CENTER_TOP),
            (HAlign::Center, VAlign::Bottom) => {
                (Pos2::new(rect.center().x, y), Align2::CENTER_BOTTOM)
            }
            (HAlign::Center, _) => (Pos2::new(rect.center().x, y), Align2::CENTER_CENTER),
            (_, VAlign::Top) => (Pos2::new(rect.min.x + CELL_PAD, y), Align2::LEFT_TOP),
            (_, VAlign::Bottom) => (Pos2::new(rect.min.x + CELL_PAD, y), Align2::LEFT_BOTTOM),
            _ => (Pos2::new(rect.min.x + CELL_PAD, y), Align2::LEFT_CENTER),
        };
        let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        let galley = clipped.layout_no_wrap(text, font, color);
        let text_rect = anchor.anchor_size(pos, galley.size());
        clipped.galley(text_rect.min, galley.clone(), color);
        if fmt.underline {
            clipped.hline(
                text_rect.min.x..=text_rect.max.x,
                text_rect.max.y - 1.0,
                Stroke::new(1.0, color),
            );
        }
        if fmt.strike {
            clipped.hline(
                text_rect.min.x..=text_rect.max.x,
                text_rect.center().y,
                Stroke::new(1.0, color),
            );
        }
    }

    fn paint_borders(&self, painter: &egui::Painter, rect: Rect, fmt: &CellFormat) {
        let edges = [
            (0, rect.left_top(), rect.right_top()),
            (1, rect.right_top(), rect.right_bottom()),
            (2, rect.left_bottom(), rect.right_bottom()),
            (3, rect.left_top(), rect.left_bottom()),
        ];
        for (i, a, b) in edges {
            if let Some(edge) = &fmt.borders[i] {
                painter.line_segment([a, b], Stroke::new(edge.width, rgb(edge.color)));
            }
        }
    }

    // ----- fill handle -----

    fn fill_handle(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        origin: Pos2,
        handle: Rect,
        st: &GridPaint,
    ) {
        painter.rect_filled(handle.shrink(1.0), 0.0, st.accent);
        painter.rect_stroke(
            handle.shrink(1.0),
            CornerRadius::ZERO,
            Stroke::new(1.0, st.bg),
            StrokeKind::Outside,
        );

        let response = ui.interact(
            handle.expand(2.0),
            Id::new("sheetz-fill-handle"),
            Sense::drag(),
        );
        if response.hovered() || self.fill_drag.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }
        if response.drag_started() {
            let sel = self.selection();
            self.fill_drag = Some(FillDrag {
                origin: sel,
                to_row: sel.r1,
                to_col: sel.c1,
            });
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (row, col) = self.cell_at(pos, origin);
                if let Some(fill) = &mut self.fill_drag {
                    fill.to_row = row.max(fill.origin.r1);
                    fill.to_col = col.max(fill.origin.c1);
                }
            }
        }
        if response.drag_stopped() {
            if let Some(fill) = self.fill_drag.take() {
                let sheet = self.sheet;
                let down = fill.to_row - fill.origin.r1;
                let right = fill.to_col - fill.origin.c1;
                // Fill follows whichever axis the pointer moved further on.
                let result = if down >= right && down > 0 {
                    self.engine.auto_fill_rows(sheet, fill.origin, fill.to_row)
                } else if right > 0 {
                    self.engine
                        .auto_fill_columns(sheet, fill.origin, fill.to_col)
                } else {
                    Ok(())
                };
                self.report("Fill", result);
                self.invalidate_geometry();
            }
        }
    }

    // ----- autofilter -----

    fn filter_buttons(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        origin: Pos2,
        rows: (i32, i32),
        cols: (i32, i32),
        st: &GridPaint,
    ) {
        let Some((header_row, f0, f1)) = self.filter.as_ref().map(|f| (f.header_row, f.c0, f.c1))
        else {
            return;
        };
        if header_row < rows.0 || header_row > rows.1 {
            return;
        }
        let mut opened = None;
        for c in f0.max(cols.0)..=f1.min(cols.1) {
            let cell = self.cell_rect(header_row, c, origin);
            if cell.width() < 20.0 || cell.height() < 8.0 {
                continue;
            }
            let button = Rect::from_min_max(
                Pos2::new(cell.max.x - 15.0, cell.min.y + 2.0),
                Pos2::new(cell.max.x - 2.0, cell.max.y - 2.0),
            );
            let response = ui.interact(button, Id::new(("sheetz-filter", c)), Sense::click());
            painter.rect_filled(button, 2.0, st.bg);
            painter.rect_stroke(button, 2.0, st.line, StrokeKind::Inside);
            let center = button.center();
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - 3.5, center.y - 2.0),
                    Pos2::new(center.x + 3.5, center.y - 2.0),
                    Pos2::new(center.x, center.y + 2.5),
                ],
                st.text,
                Stroke::NONE,
            ));
            if response.clicked() {
                opened = Some(c);
            }
        }
        if let (Some(c), Some(filter)) = (opened, self.filter.as_mut()) {
            filter.open_column = Some(c);
        }
    }

    // ----- frozen panes -----

    /// Frozen rows/columns are re-painted over the scrolled content, pinned to
    /// the grid origin on the frozen axis.
    fn frozen_overlay(&mut self, ui: &mut Ui, grid_rect: Rect, offset: Vec2) {
        let (frozen_rows, frozen_cols) = self.engine.frozen(self.sheet);
        let fr = frozen_rows.clamp(0, self.view_rows);
        let fc = frozen_cols.clamp(0, self.view_cols);
        if fr == 0 && fc == 0 {
            return;
        }
        let st = self.paint_style(ui);
        let merges = self.engine.merges(self.sheet);
        let split_x = grid_rect.min.x + self.col_off[fc as usize];
        let split_y = grid_rect.min.y + self.row_off[fr as usize];
        // Origins that pin one axis to the grid and let the other scroll.
        let pinned = grid_rect.min;
        let scroll_x = Pos2::new(grid_rect.min.x - offset.x, grid_rect.min.y);
        let scroll_y = Pos2::new(grid_rect.min.x, grid_rect.min.y - offset.y);
        let (vr0, vr1) = self.visible_rows(offset.y, grid_rect.height());
        let (vc0, vc1) = self.visible_cols(offset.x, grid_rect.width());

        // (id, band rect, origin, rows, cols)
        let bands = [
            (
                "sheetz-frozen-top",
                Rect::from_min_max(Pos2::new(split_x, grid_rect.min.y), Pos2::new(grid_rect.max.x, split_y)),
                scroll_x,
                (1, fr),
                (vc0.max(fc + 1), vc1),
            ),
            (
                "sheetz-frozen-left",
                Rect::from_min_max(Pos2::new(grid_rect.min.x, split_y), Pos2::new(split_x, grid_rect.max.y)),
                scroll_y,
                (vr0.max(fr + 1), vr1),
                (1, fc),
            ),
            (
                "sheetz-frozen-corner",
                Rect::from_min_max(grid_rect.min, Pos2::new(split_x, split_y)),
                pinned,
                (1, fr),
                (1, fc),
            ),
        ];

        let shift = ui.input(|i| i.modifiers.shift);
        for (id, band, origin, rows, cols) in bands {
            let band = band.intersect(grid_rect);
            if !band.is_positive() || rows.1 < rows.0 || cols.1 < cols.0 {
                continue;
            }
            let painter = ui.painter().with_clip_rect(band);
            painter.rect_filled(band, 0.0, st.bg);
            self.paint_block(&painter, origin, rows, cols, &st, &merges);

            let response = ui.interact(band, Id::new(id), Sense::click_and_drag());
            if let Some(pos) = response.interact_pointer_pos() {
                let cell = self.cell_at(pos, origin);
                let merge = merged_at(&merges, cell.0, cell.1);
                if response.double_clicked() {
                    let (row, col) = merge.map_or(cell, |m| (m.r0, m.c0));
                    self.set_cursor(row, col);
                    let text = self.engine.content(self.sheet, row, col);
                    self.begin_edit(text, EditMode::Edit);
                } else if response.drag_started() || response.clicked() {
                    self.select_cell(cell, merge, shift);
                } else if response.dragged() {
                    self.cursor = merge.map_or(cell, |m| (m.r0, m.c0));
                }
            }
        }

        // Freeze boundaries read as a stronger line than a normal grid line.
        let split = Stroke::new(2.0, st.text.gamma_multiply(0.45));
        let painter = ui.painter().with_clip_rect(grid_rect);
        if fr > 0 {
            painter.hline(grid_rect.min.x..=grid_rect.max.x, split_y, split);
        }
        if fc > 0 {
            painter.vline(split_x, grid_rect.min.y..=grid_rect.max.y, split);
        }
    }

    // ----- context menu -----

    fn cell_context_menu(&mut self, ui: &mut Ui, cmds: &mut Vec<Command>) {
        let click = |ui: &mut Ui, label: &str, cmd: Command, cmds: &mut Vec<Command>| {
            if ui.button(label).clicked() {
                cmds.push(cmd);
                ui.close();
            }
        };
        click(ui, "Cut", Command::Cut, cmds);
        click(ui, "Copy", Command::Copy, cmds);
        click(ui, "Paste", Command::Paste, cmds);
        click(ui, "Paste values only", Command::PasteValues, cmds);
        ui.separator();
        click(ui, "Insert row", Command::InsertRow, cmds);
        click(ui, "Insert column", Command::InsertColumn, cmds);
        click(ui, "Delete row", Command::DeleteRow, cmds);
        click(ui, "Delete column", Command::DeleteColumn, cmds);
        ui.separator();
        click(ui, "Clear contents", Command::Clear, cmds);
        click(ui, "Clear formatting", Command::ClearFormats, cmds);
        ui.separator();
        click(ui, "Format cells…", Command::FormatDialog, cmds);
    }

    // ----- headers -----

    fn headers(
        &mut self,
        ui: &mut Ui,
        avail: Rect,
        grid_rect: Rect,
        offset: Vec2,
        cmds: &mut Vec<Command>,
    ) {
        // A resize ends with the mouse button, even if the pointer left the header.
        if self.resizing.is_some() && !ui.input(|i| i.pointer.any_down()) {
            self.resizing = None;
        }

        let visuals = ui.visuals().clone();
        let bg = visuals.faint_bg_color;
        let line = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
        let font = FontId::proportional(12.0);
        let text_color = visuals.text_color();
        let accent = visuals.selection.bg_fill;
        let sel = self.selection();
        let origin_x = grid_rect.min.x - offset.x;
        let origin_y = grid_rect.min.y - offset.y;

        // Column headers.
        let col_hdr = Rect::from_min_max(
            Pos2::new(grid_rect.min.x, avail.min.y),
            Pos2::new(avail.max.x, avail.min.y + HEADER_H),
        );
        let painter = ui.painter().with_clip_rect(col_hdr);
        painter.rect_filled(col_hdr, 0.0, bg);
        let (c0, c1) = self.visible_cols(offset.x, col_hdr.width());
        for c in c0..=c1 {
            let x0 = origin_x + self.col_off[c as usize - 1];
            let x1 = origin_x + self.col_off[c as usize];
            let rect = Rect::from_min_max(Pos2::new(x0, col_hdr.min.y), Pos2::new(x1, col_hdr.max.y));
            if c >= sel.c0 && c <= sel.c1 {
                painter.rect_filled(rect, 0.0, accent.linear_multiply(0.25));
            }
            painter.vline(x1, col_hdr.min.y..=col_hdr.max.y, line);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                column_name(c),
                font.clone(),
                text_color,
            );
        }
        painter.hline(col_hdr.min.x..=col_hdr.max.x, col_hdr.max.y, line);

        // Row headers.
        let row_hdr = Rect::from_min_max(
            Pos2::new(avail.min.x, grid_rect.min.y),
            Pos2::new(avail.min.x + GUTTER_W, avail.max.y),
        );
        let painter = ui.painter().with_clip_rect(row_hdr);
        painter.rect_filled(row_hdr, 0.0, bg);
        let (r0, r1) = self.visible_rows(offset.y, row_hdr.height());
        for r in r0..=r1 {
            let y0 = origin_y + self.row_off[r as usize - 1];
            let y1 = origin_y + self.row_off[r as usize];
            if (y1 - y0).abs() < 0.5 {
                continue; // hidden row
            }
            let rect = Rect::from_min_max(Pos2::new(row_hdr.min.x, y0), Pos2::new(row_hdr.max.x, y1));
            if r >= sel.r0 && r <= sel.r1 {
                painter.rect_filled(rect, 0.0, accent.linear_multiply(0.25));
            }
            painter.hline(row_hdr.min.x..=row_hdr.max.x, y1, line);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                r.to_string(),
                font.clone(),
                text_color,
            );
        }
        painter.vline(row_hdr.max.x, row_hdr.min.y..=row_hdr.max.y, line);

        // Corner box: click selects the whole sheet.
        let corner = Rect::from_min_size(avail.min, Vec2::new(GUTTER_W, HEADER_H));
        let painter = ui.painter().with_clip_rect(corner);
        painter.rect_filled(corner, 0.0, bg);
        painter.hline(corner.min.x..=corner.max.x, corner.max.y, line);
        painter.vline(corner.max.x, corner.min.y..=corner.max.y, line);

        self.column_header_input(ui, col_hdr, origin_x, (c0, c1), cmds);
        self.row_header_input(ui, row_hdr, origin_y, (r0, r1));
        if ui
            .interact(corner, Id::new("sheetz-corner"), Sense::click())
            .clicked()
        {
            self.cursor = (1, 1);
            self.anchor = (self.view_rows, self.view_cols);
        }
    }

    /// The column whose right edge is within the grab zone of `x`.
    fn col_edge_at(&self, x: f32, origin_x: f32, c0: i32, c1: i32) -> Option<i32> {
        ((c0 - 1).max(1)..=c1).find(|&c| (origin_x + self.col_off[c as usize] - x).abs() <= RESIZE_GRAB)
    }

    fn row_edge_at(&self, y: f32, origin_y: f32, r0: i32, r1: i32) -> Option<i32> {
        ((r0 - 1).max(1)..=r1).find(|&r| (origin_y + self.row_off[r as usize] - y).abs() <= RESIZE_GRAB)
    }

    fn column_header_input(
        &mut self,
        ui: &mut Ui,
        col_hdr: Rect,
        origin_x: f32,
        visible: (i32, i32),
        cmds: &mut Vec<Command>,
    ) {
        let response = ui.interact(
            col_hdr,
            Id::new("sheetz-col-header"),
            Sense::click_and_drag(),
        );
        let resizing = matches!(self.resizing, Some(Resizing::Column { .. }));
        let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) else {
            return;
        };
        let edge = self.col_edge_at(pos.x, origin_x, visible.0, visible.1);
        if edge.is_some() || resizing {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        if response.double_clicked() {
            if let Some(c) = edge {
                self.cursor = (1, c);
                self.anchor = (self.view_rows, c);
                cmds.push(Command::AutofitColumn);
            }
            return;
        }
        let col = self.col_at_x(pos.x, origin_x);
        if response.drag_started() {
            match edge {
                Some(c) => {
                    let start_w = self.engine.col_width(self.sheet, c);
                    self.resizing = Some(Resizing::Column {
                        index: c,
                        start_x: pos.x,
                        start_w,
                    });
                }
                None => {
                    self.cursor = (1, col);
                    self.anchor = (self.view_rows, col);
                }
            }
        } else if response.clicked() && edge.is_none() {
            let shift = ui.input(|i| i.modifiers.shift);
            self.cursor = (1, col);
            if !shift {
                self.anchor = (self.view_rows, col);
            } else {
                self.anchor = (self.view_rows, self.anchor.1);
            }
        } else if response.dragged() {
            if let Some(Resizing::Column {
                index,
                start_x,
                start_w,
            }) = self.resizing
            {
                let width = (start_w + (pos.x - start_x) / self.zoom).max(0.0) as f64;
                let (sheet, index) = (self.sheet, index);
                let result = self.engine.set_col_width(sheet, index, index, width);
                self.report("Column width", result);
                self.invalidate_geometry();
            } else {
                self.cursor = (1, col);
            }
        }
        if response.drag_stopped() && resizing {
            self.resizing = None;
        }
    }

    fn row_header_input(&mut self, ui: &mut Ui, row_hdr: Rect, origin_y: f32, visible: (i32, i32)) {
        let response = ui.interact(
            row_hdr,
            Id::new("sheetz-row-header"),
            Sense::click_and_drag(),
        );
        let resizing = matches!(self.resizing, Some(Resizing::Row { .. }));
        let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) else {
            return;
        };
        let edge = self.row_edge_at(pos.y, origin_y, visible.0, visible.1);
        if edge.is_some() || resizing {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        }

        let row = self.row_at_y(pos.y, origin_y);
        if response.drag_started() {
            match edge {
                Some(r) => {
                    let start_h = self.engine.row_height(self.sheet, r);
                    self.resizing = Some(Resizing::Row {
                        index: r,
                        start_y: pos.y,
                        start_h,
                    });
                }
                None => {
                    self.cursor = (row, 1);
                    self.anchor = (row, self.view_cols);
                }
            }
        } else if response.clicked() && edge.is_none() {
            let shift = ui.input(|i| i.modifiers.shift);
            self.cursor = (row, 1);
            if !shift {
                self.anchor = (row, self.view_cols);
            } else {
                self.anchor = (self.anchor.0, self.view_cols);
            }
        } else if response.dragged() {
            if let Some(Resizing::Row {
                index,
                start_y,
                start_h,
            }) = self.resizing
            {
                let height = (start_h + (pos.y - start_y) / self.zoom).max(0.0) as f64;
                let (sheet, index) = (self.sheet, index);
                let result = self.engine.set_row_height(sheet, index, index, height);
                self.report("Row height", result);
                self.invalidate_geometry();
            } else {
                self.cursor = (row, 1);
            }
        }
        if response.drag_stopped() && resizing {
            self.resizing = None;
        }
    }
}

/// A1-style column name for a 1-based column index.
pub fn column_name(mut col: i32) -> String {
    let mut name = String::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        name.insert(0, (b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    name
}

/// Human-readable reference for a range, e.g. "B2" or "B2:D9".
pub fn range_name(range: Range) -> String {
    if range.rows() == 1 && range.cols() == 1 {
        format!("{}{}", column_name(range.c0), range.r0)
    } else {
        format!(
            "{}{}:{}{}",
            column_name(range.c0),
            range.r0,
            column_name(range.c1),
            range.r1
        )
    }
}

#[cfg(test)]
mod tests {
    use super::column_name;

    #[test]
    fn column_names_follow_the_a1_convention() {
        assert_eq!(column_name(1), "A");
        assert_eq!(column_name(26), "Z");
        assert_eq!(column_name(27), "AA");
        assert_eq!(column_name(702), "ZZ");
        assert_eq!(column_name(703), "AAA");
    }
}
