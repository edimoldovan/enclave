//! Native chart rendering and the conditional-formatting dialog.
//!
//! Charts are drawn with egui's painter — no plotting library, no xlsx chart
//! XML (charts are a view over a range and are not saved into the workbook).

use eframe::egui::{self, Align2, Context, Key};

use crate::state::{Chart, ChartKind, Dialog, GridoApp};

impl GridoApp {
    pub fn cf_dialog_impl(&mut self, ctx: &Context) {
        let mut open = true;
        egui::Window::new("Conditional formatting")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Rules apply to the current selection.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.cf.kind, 0, "Cell value");
                    ui.selectable_value(&mut self.cf.kind, 1, "Color scale");
                });
                ui.add_space(6.0);
                if self.cf.kind == 0 {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("cf-op")
                            .selected_text(
                                ["greater than", "less than", "equal to", "between"]
                                    [self.cf.operator],
                            )
                            .show_ui(ui, |ui| {
                                for (i, label) in ["greater than", "less than", "equal to", "between"]
                                    .iter()
                                    .enumerate()
                                {
                                    ui.selectable_value(&mut self.cf.operator, i, *label);
                                }
                            });
                        ui.text_edit_singleline(&mut self.cf.value);
                        if self.cf.operator == 3 {
                            ui.label("and");
                            ui.text_edit_singleline(&mut self.cf.value2);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Fill:");
                        ui.color_edit_button_srgb(&mut self.cf.fill);
                    });
                }
                ui.add_space(8.0);
                if ui.button("Add rule").clicked() {
                    let range = self.selection();
                    let sheet = self.sheet;
                    let result = if self.cf.kind == 1 {
                        self.engine.add_cf_color_scale(sheet, range)
                    } else {
                        let op = ["greater", "less", "equal", "between"][self.cf.operator];
                        let hex = format!(
                            "#{:02X}{:02X}{:02X}",
                            self.cf.fill[0], self.cf.fill[1], self.cf.fill[2]
                        );
                        let second = if self.cf.operator == 3 {
                            Some(self.cf.value2.as_str())
                        } else {
                            None
                        };
                        let value = self.cf.value.clone();
                        self.engine
                            .add_cf_cell_is(sheet, range, op, &value, second, &hex)
                    };
                    match result {
                        Ok(()) => self.cf.message = "Rule added".to_string(),
                        Err(e) => self.cf.message = format!("{e}"),
                    }
                    self.invalidate_geometry();
                }
                if !self.cf.message.is_empty() {
                    ui.label(egui::RichText::new(&self.cf.message).weak());
                }

                ui.separator();
                ui.label("Existing rules");
                let rules = self.engine.cf_rules(self.sheet);
                if rules.is_empty() {
                    ui.label(egui::RichText::new("None on this sheet.").weak());
                }
                let mut delete: Option<u32> = None;
                for (index, range, kind) in &rules {
                    ui.horizontal(|ui| {
                        ui.label(format!("{kind} — {range}"));
                        if ui.small_button("🗑").clicked() {
                            delete = Some(*index);
                        }
                    });
                }
                if let Some(index) = delete {
                    let sheet = self.sheet;
                    let result = self.engine.delete_cf_rule(sheet, index);
                    self.report("Delete rule", result);
                    self.invalidate_geometry();
                }
            });
        if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    pub fn chart_dialog_impl(&mut self, ctx: &Context) {
        let mut open = true;
        let mut create = false;
        egui::Window::new("Insert chart")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let sel = self.selection();
                ui.label(format!(
                    "Data: {} ({}×{})",
                    crate::ui::grid::range_name(sel),
                    sel.rows(),
                    sel.cols()
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.chart_draft, ChartKind::Bar, "Bar");
                    ui.selectable_value(&mut self.chart_draft, ChartKind::Line, "Line");
                    ui.selectable_value(&mut self.chart_draft, ChartKind::Pie, "Pie");
                });
                ui.add_space(8.0);
                if sel.rows() < 2 && sel.cols() < 2 {
                    ui.label(
                        egui::RichText::new("Select a range with data first.")
                            .weak(),
                    );
                } else if ui.button("Create chart").clicked() {
                    create = true;
                }
            });
        if create {
            let sel = self.selection();
            let has_header = (sel.c0..=sel.c1)
                .any(|c| !self.engine.is_number(self.sheet, sel.r0, c));
            self.charts.push(Chart {
                kind: self.chart_draft,
                sheet: self.sheet,
                data: sel,
                title: format!("Chart {}", self.charts.len() + 1),
                has_header,
            });
            self.dialog = Dialog::None;
        } else if !open || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dialog = Dialog::None;
        }
    }

    /// Floating windows for each created chart.
    pub fn chart_windows(&mut self, ctx: &Context) {
        let charts: Vec<(usize, ChartKind, crate::engine::Range, String, u32, bool)> = self
            .charts
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.kind, c.data, c.title.clone(), c.sheet, c.has_header))
            .collect();
        let mut close: Option<usize> = None;
        for (index, kind, data, title, sheet, has_header) in charts {
            if sheet != self.sheet {
                continue;
            }
            let mut open = true;
            egui::Window::new(title)
                .open(&mut open)
                .default_size([420.0, 280.0])
                .show(ctx, |ui| {
                    self.paint_chart(ui, kind, data, sheet, has_header);
                });
            if !open {
                close = Some(index);
            }
        }
        if let Some(index) = close {
            self.charts.remove(index);
        }
    }

    fn paint_chart(
        &self,
        ui: &mut egui::Ui,
        kind: ChartKind,
        data: crate::engine::Range,
        sheet: u32,
        has_header: bool,
    ) {
        // Series: use the last column of numbers; labels from the first column.
        let first_data_row = if has_header { data.r0 + 1 } else { data.r0 };
        let label_col = data.c0;
        let value_col = if data.cols() > 1 { data.c1 } else { data.c0 };

        let mut points: Vec<(String, f64)> = Vec::new();
        for r in first_data_row..=data.r1 {
            let raw = self.engine.display(sheet, r, value_col);
            let Ok(value) = raw.replace(',', "").replace('$', "").trim().parse::<f64>() else {
                continue;
            };
            let label = if data.cols() > 1 {
                self.engine.display(sheet, r, label_col)
            } else {
                format!("{r}")
            };
            points.push((label, value));
        }

        if points.is_empty() {
            ui.label("No numeric data in the selected range.");
            return;
        }

        let palette = [
            egui::Color32::from_rgb(74, 134, 232),
            egui::Color32::from_rgb(224, 123, 57),
            egui::Color32::from_rgb(106, 168, 79),
            egui::Color32::from_rgb(204, 65, 37),
            egui::Color32::from_rgb(142, 124, 195),
            egui::Color32::from_rgb(212, 172, 54),
        ];
        let size = ui.available_size_before_wrap();
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        let plot = rect.shrink2(egui::vec2(28.0, 22.0));
        let text_color = ui.visuals().text_color();
        let axis = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
        let font = egui::FontId::proportional(10.0);

        match kind {
            ChartKind::Pie => {
                let total: f64 = points.iter().map(|(_, v)| v.abs()).sum();
                if total <= 0.0 {
                    ui.label("Values sum to zero.");
                    return;
                }
                let center = plot.center();
                let radius = plot.height().min(plot.width()) * 0.42;
                let mut start = -std::f32::consts::FRAC_PI_2;
                for (i, (label, value)) in points.iter().enumerate() {
                    let frac = (value.abs() / total) as f32;
                    let sweep = frac * std::f32::consts::TAU;
                    let color = palette[i % palette.len()];
                    // Approximate the wedge with a triangle fan.
                    let steps = (sweep / 0.12).ceil().max(2.0) as usize;
                    let mut mesh = egui::Mesh::default();
                    for s in 0..=steps {
                        let angle = start + sweep * (s as f32 / steps as f32);
                        let p = center + egui::vec2(angle.cos(), angle.sin()) * radius;
                        mesh.colored_vertex(p, color);
                    }
                    mesh.colored_vertex(center, color);
                    let hub = mesh.vertices.len() as u32 - 1;
                    for s in 0..steps as u32 {
                        mesh.add_triangle(hub, s, s + 1);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    let mid = start + sweep / 2.0;
                    let label_pos =
                        center + egui::vec2(mid.cos(), mid.sin()) * (radius + 14.0);
                    painter.text(
                        label_pos,
                        Align2::CENTER_CENTER,
                        format!("{label} {:.0}%", frac * 100.0),
                        font.clone(),
                        text_color,
                    );
                    start += sweep;
                }
            }
            ChartKind::Bar | ChartKind::Line => {
                let max = points.iter().map(|(_, v)| *v).fold(f64::MIN, f64::max);
                let min = points.iter().map(|(_, v)| *v).fold(f64::MAX, f64::min);
                let top = max.max(0.0);
                let bottom = min.min(0.0);
                let span = (top - bottom).max(1e-9);
                let y_of = |v: f64| -> f32 {
                    plot.max.y - ((v - bottom) / span) as f32 * plot.height()
                };
                // Axes.
                painter.line_segment(
                    [
                        egui::pos2(plot.min.x, y_of(0.0)),
                        egui::pos2(plot.max.x, y_of(0.0)),
                    ],
                    axis,
                );
                painter.line_segment([plot.left_top(), plot.left_bottom()], axis);
                painter.text(
                    plot.left_top() + egui::vec2(-4.0, 0.0),
                    Align2::RIGHT_TOP,
                    format!("{top:.0}"),
                    font.clone(),
                    text_color,
                );

                let n = points.len();
                let slot = plot.width() / n as f32;
                if kind == ChartKind::Bar {
                    for (i, (label, value)) in points.iter().enumerate() {
                        let x = plot.min.x + slot * i as f32;
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(x + slot * 0.15, y_of(value.max(0.0))),
                            egui::pos2(x + slot * 0.85, y_of(value.min(0.0))),
                        );
                        painter.rect_filled(bar, 2.0, palette[i % palette.len()]);
                        painter.text(
                            egui::pos2(x + slot * 0.5, plot.max.y + 3.0),
                            Align2::CENTER_TOP,
                            elide(label, 8),
                            font.clone(),
                            text_color,
                        );
                    }
                } else {
                    let pts: Vec<egui::Pos2> = points
                        .iter()
                        .enumerate()
                        .map(|(i, (_, v))| {
                            egui::pos2(plot.min.x + slot * (i as f32 + 0.5), y_of(*v))
                        })
                        .collect();
                    painter.add(egui::Shape::line(
                        pts.clone(),
                        egui::Stroke::new(2.0, palette[0]),
                    ));
                    for (i, p) in pts.iter().enumerate() {
                        painter.circle_filled(*p, 3.0, palette[0]);
                        painter.text(
                            egui::pos2(p.x, plot.max.y + 3.0),
                            Align2::CENTER_TOP,
                            elide(&points[i].0, 8),
                            font.clone(),
                            text_color,
                        );
                    }
                }
            }
        }
    }
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
