//! Spreadsheet engine wrapper.
//!
//! Everything the UI needs from IronCalc goes through here, expressed in plain
//! types (bools, Strings, [u8; 3] colors). UI modules never import ironcalc.

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ironcalc::base::expressions::types::Area;
use ironcalc::base::types::{CellType, Color, HorizontalAlignment, VerticalAlignment};
use ironcalc::base::UserModel;
use ironcalc::export::save_xlsx_to_writer;
use ironcalc::import::load_from_xlsx;

/// Sheet limits, as the xlsx format defines them.
pub const MAX_ROWS: i32 = 1_048_576;
pub const MAX_COLS: i32 = 16_384;

const LOCALE: &str = "en";
const TZ: &str = "UTC";
const LANG: &str = "en";

/// A rectangular block of cells, 1-based inclusive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Range {
    pub r0: i32,
    pub r1: i32,
    pub c0: i32,
    pub c1: i32,
}

impl Range {
    pub fn single(row: i32, col: i32) -> Range {
        Range {
            r0: row,
            r1: row,
            c0: col,
            c1: col,
        }
    }

    pub fn rows(&self) -> i32 {
        self.r1 - self.r0 + 1
    }

    pub fn cols(&self) -> i32 {
        self.c1 - self.c0 + 1
    }

    pub fn contains(&self, row: i32, col: i32) -> bool {
        row >= self.r0 && row <= self.r1 && col >= self.c0 && col <= self.c1
    }

    fn area(&self, sheet: u32) -> Area {
        Area {
            sheet,
            row: self.r0,
            column: self.c0,
            width: self.cols(),
            height: self.rows(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

/// One cell border edge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BorderEdge {
    pub color: [u8; 3],
    /// Rendering width in points; thick styles get a fatter line.
    pub width: f32,
}

/// A conditional-formatting data bar to paint behind a cell's text.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DataBar {
    pub color: [u8; 3],
    /// Proportion of the cell width to fill, 0..1.
    pub fill: f32,
    /// Where the zero axis sits across the cell, 0..1.
    pub axis: f32,
}

/// Everything the grid needs to paint one cell, resolved to plain values.
#[derive(Clone, PartialEq, Debug)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub font_size: f32,
    pub font_color: Option<[u8; 3]>,
    pub fill_color: Option<[u8; 3]>,
    pub align: HAlign,
    pub valign: VAlign,
    pub wrap: bool,
    pub num_fmt: String,
    /// top, right, bottom, left
    pub borders: [Option<BorderEdge>; 4],
    /// Set when a conditional-formatting data bar applies.
    pub data_bar: Option<DataBar>,
}

impl Default for CellFormat {
    fn default() -> Self {
        CellFormat {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            font_size: 11.0,
            font_color: None,
            fill_color: None,
            align: HAlign::General,
            valign: VAlign::Center,
            wrap: false,
            num_fmt: "general".to_string(),
            borders: [None, None, None, None],
            data_bar: None,
        }
    }
}

/// Style attributes settable on a range. Maps to IronCalc "style paths".
#[derive(Clone, Debug)]
pub enum StyleEdit {
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strike(bool),
    FontSize(i32),
    FontColor(Option<String>),
    FillColor(Option<String>),
    Align(HAlign),
    VAlign(VAlign),
    Wrap(bool),
    NumFmt(String),
}

impl StyleEdit {
    fn path_value(&self) -> (&'static str, String) {
        match self {
            StyleEdit::Bold(v) => ("font.b", v.to_string()),
            StyleEdit::Italic(v) => ("font.i", v.to_string()),
            StyleEdit::Underline(v) => ("font.u", v.to_string()),
            StyleEdit::Strike(v) => ("font.strike", v.to_string()),
            StyleEdit::FontSize(v) => ("font.size", v.to_string()),
            StyleEdit::FontColor(v) => ("font.color", v.clone().unwrap_or_default()),
            StyleEdit::FillColor(v) => ("fill.color", v.clone().unwrap_or_default()),
            StyleEdit::Align(v) => (
                "alignment.horizontal",
                match v {
                    HAlign::General => "general",
                    HAlign::Left => "left",
                    HAlign::Center => "center",
                    HAlign::Right => "right",
                }
                .to_string(),
            ),
            StyleEdit::VAlign(v) => (
                "alignment.vertical",
                match v {
                    VAlign::Top => "top",
                    VAlign::Center => "center",
                    VAlign::Bottom => "bottom",
                }
                .to_string(),
            ),
            StyleEdit::Wrap(v) => ("alignment.wrap_text", v.to_string()),
            StyleEdit::NumFmt(v) => ("num_fmt", v.clone()),
        }
    }
}

/// Which edges a border command applies to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorderTarget {
    All,
    Outer,
    Inner,
    Top,
    Right,
    Bottom,
    Left,
    None,
}

/// A match from Find.
#[derive(Clone, Debug)]
pub struct FindHit {
    pub sheet: u32,
    pub row: i32,
    pub col: i32,
}

/// Thin wrapper around IronCalc's `UserModel` (evaluation, undo/redo history,
/// edit semantics) plus file-path/dirty bookkeeping.
pub struct Engine {
    um: UserModel<'static>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    clipboard: Option<InternalClip>,
    /// Undo entries produced since `begin_transaction`.
    undo_steps: usize,
}

/// A formula-aware copy taken from this app (survives across sheets).
struct InternalClip {
    sheet: u32,
    range: (i32, i32, i32, i32),
    data: serde_json::Value,
    csv: String,
}

fn hex_to_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() < 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

impl Engine {
    pub fn new() -> Result<Engine> {
        let um = UserModel::new_empty("Book1", LOCALE, TZ, LANG).map_err(|e| anyhow!(e))?;
        Ok(Engine {
            um,
            path: None,
            dirty: false,
            clipboard: None,
            undo_steps: 0,
        })
    }

    pub fn open(path: &Path) -> Result<Engine> {
        let file = path
            .to_str()
            .ok_or_else(|| anyhow!("path is not valid UTF-8"))?;
        let model = load_from_xlsx(file, LOCALE, TZ, LANG).map_err(|e| anyhow!("{e:?}"))?;
        Ok(Engine {
            um: UserModel::from_model(model),
            path: Some(path.to_path_buf()),
            dirty: false,
            clipboard: None,
            undo_steps: 0,
        })
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        // ironcalc's save_to_xlsx refuses to overwrite, so write through
        // save_xlsx_to_writer with a normal (truncating) File::create.
        let file = std::fs::File::create(path)?;
        save_xlsx_to_writer(self.um.get_model(), BufWriter::new(file))
            .map_err(|e| anyhow!("{e:?}"))?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    // ----- reading cells -----

    /// Value as shown in the grid (number formats applied).
    pub fn display(&self, sheet: u32, row: i32, col: i32) -> String {
        self.um
            .get_formatted_cell_value(sheet, row, col)
            .unwrap_or_default()
    }

    /// Raw cell content as typed by the user (formulas keep their `=`).
    pub fn content(&self, sheet: u32, row: i32, col: i32) -> String {
        self.um.get_cell_content(sheet, row, col).unwrap_or_default()
    }

    pub fn is_number(&self, sheet: u32, row: i32, col: i32) -> bool {
        matches!(self.um.get_cell_type(sheet, row, col), Ok(CellType::Number))
    }

    pub fn is_error(&self, sheet: u32, row: i32, col: i32) -> bool {
        matches!(
            self.um.get_cell_type(sheet, row, col),
            Ok(CellType::ErrorValue)
        )
    }

    pub fn is_empty(&self, sheet: u32, row: i32, col: i32) -> bool {
        self.content(sheet, row, col).is_empty()
    }

    fn resolve(&self, color: &Color) -> Option<[u8; 3]> {
        match color {
            Color::None => None,
            other => hex_to_rgb(&self.um.resolve_color(other)),
        }
    }

    /// Resolved formatting for one cell, ready to paint. Uses the *extended*
    /// style so conditional-formatting overlays (dxf fills, color scales, data
    /// bars) are already applied.
    pub fn format(&self, sheet: u32, row: i32, col: i32) -> CellFormat {
        let Ok(extended) = self.um.get_extended_cell_style(sheet, row, col) else {
            return CellFormat::default();
        };
        let style = extended.style;
        let data_bar = extended.data_bar.as_ref().map(|bar| DataBar {
            color: self.resolve(&bar.positive_color).unwrap_or([80, 140, 220]),
            fill: bar.value.clamp(0.0, 1.0) as f32,
            axis: bar.axis_position.clamp(0.0, 1.0) as f32,
        });
        let (align, valign, wrap) = match &style.alignment {
            Some(a) => (
                match a.horizontal {
                    HorizontalAlignment::Left => HAlign::Left,
                    HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous => {
                        HAlign::Center
                    }
                    HorizontalAlignment::Right => HAlign::Right,
                    _ => HAlign::General,
                },
                match a.vertical {
                    VerticalAlignment::Top => VAlign::Top,
                    VerticalAlignment::Bottom => VAlign::Bottom,
                    _ => VAlign::Center,
                },
                a.wrap_text,
            ),
            None => (HAlign::General, VAlign::Center, false),
        };
        let edge = |item: &Option<ironcalc::base::types::BorderItem>| {
            item.as_ref().map(|b| {
                use ironcalc::base::types::BorderStyle::*;
                let width = match b.style {
                    Thin | Dotted => 1.0,
                    Medium | MediumDashed | MediumDashDot | MediumDashDotDot | SlantDashDot => 1.8,
                    Thick | Double => 2.6,
                };
                BorderEdge {
                    color: self.resolve(&b.color).unwrap_or([90, 90, 90]),
                    width,
                }
            })
        };
        CellFormat {
            bold: style.font.b,
            italic: style.font.i,
            underline: style.font.u,
            strike: style.font.strike,
            font_size: style.font.sz as f32,
            font_color: self.resolve(&style.font.color),
            fill_color: self.resolve(&style.fill.color),
            align,
            valign,
            wrap,
            num_fmt: style.num_fmt.clone(),
            borders: [
                edge(&style.border.top),
                edge(&style.border.right),
                edge(&style.border.bottom),
                edge(&style.border.left),
            ],
            data_bar,
        }
    }

    // ----- conditional formatting -----

    /// (index, range, human-readable kind) for each rule on the sheet.
    pub fn cf_rules(&self, sheet: u32) -> Vec<(u32, String, String)> {
        self.um
            .get_conditional_formatting_list(sheet)
            .unwrap_or_default()
            .into_iter()
            .map(|view| {
                let kind = format!("{:?}", view.cf_rule);
                let kind = kind.split_whitespace().next().unwrap_or("Rule").to_string();
                (view.index as u32, view.range, kind)
            })
            .collect()
    }

    pub fn delete_cf_rule(&mut self, sheet: u32, index: u32) -> Result<()> {
        self.um
            .delete_conditional_formatting(sheet, index)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    /// Adds a "cell value <op> value" rule that fills matching cells.
    /// `op` is one of: greater, less, equal, between.
    pub fn add_cf_cell_is(
        &mut self,
        sheet: u32,
        range: Range,
        op: &str,
        formula: &str,
        formula2: Option<&str>,
        fill_hex: &str,
    ) -> Result<()> {
        let range_str = a1_range(range);
        let operator = match op {
            "greater" => "GreaterThan",
            "less" => "LessThan",
            "equal" => "Equal",
            "between" => "Between",
            other => return Err(anyhow!("unknown operator {other}")),
        };
        // CfRuleInput is internally tagged (`#[serde(tag = "type")]`).
        let rule = serde_json::json!({
            "type": "CellIs",
            "operator": operator,
            "formula": formula,
            "formula2": formula2,
            "format": {
                "font": null,
                "fill": { "color": fill_hex },
                "border": null,
                "num_fmt": null,
                "alignment": null,
            },
            "stop_if_true": false,
        });
        let rule: ironcalc::base::cf_types::CfRuleInput =
            serde_json::from_value(rule).map_err(|e| anyhow!("rule: {e}"))?;
        self.um
            .add_conditional_formatting(sheet, &range_str, rule)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    /// Adds a two-color scale over the range.
    pub fn add_cf_color_scale(&mut self, sheet: u32, range: Range) -> Result<()> {
        let range_str = a1_range(range);
        let rule = serde_json::json!({
            "type": "ColorScale",
            "thresholds": [
                { "cfvo": "Min", "color": "#FFFFFF" },
                { "cfvo": "Max", "color": "#63BE7B" },
            ],
        });
        let rule: ironcalc::base::cf_types::CfRuleInput =
            serde_json::from_value(rule).map_err(|e| anyhow!("rule: {e}"))?;
        self.um
            .add_conditional_formatting(sheet, &range_str, rule)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    // ----- writing cells -----

    pub fn set_input(&mut self, sheet: u32, row: i32, col: i32, value: &str) -> Result<()> {
        self.um
            .set_user_input(sheet, row, col, value)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn clear_contents(&mut self, sheet: u32, range: Range) -> Result<()> {
        self.um
            .range_clear_contents(&range.area(sheet))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn clear_formatting(&mut self, sheet: u32, range: Range) -> Result<()> {
        self.um
            .range_clear_formatting(&range.area(sheet))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn clear_all(&mut self, sheet: u32, range: Range) -> Result<()> {
        self.um
            .range_clear_all(&range.area(sheet))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn set_style(&mut self, sheet: u32, range: Range, edit: StyleEdit) -> Result<()> {
        let (path, value) = edit.path_value();
        // Dragging a color picker fires every frame; skip no-op writes so the
        // undo stack doesn't fill with identical entries.
        if range.rows() == 1 && range.cols() == 1 {
            if let Ok(current) = self.um.get_cell_style(sheet, range.r0, range.c0) {
                if style_value_matches(&current, path, &value) {
                    return Ok(());
                }
            }
        }
        self.um
            .update_range_style(&range.area(sheet), path, &value)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    /// Applies several style edits as one undo step.
    pub fn set_styles(&mut self, sheet: u32, range: Range, edits: &[StyleEdit]) -> Result<()> {
        for edit in edits {
            self.set_style(sheet, range, edit.clone())?;
        }
        Ok(())
    }

    /// Applies a border to a range. `width` 0 removes borders.
    pub fn set_border(
        &mut self,
        sheet: u32,
        range: Range,
        target: BorderTarget,
        color: &str,
    ) -> Result<()> {
        // BorderArea's fields are pub(crate) but it is Deserialize, so build it
        // from its serde representation.
        let type_name = match target {
            BorderTarget::All => "All",
            BorderTarget::Outer => "Outer",
            BorderTarget::Inner => "Inner",
            BorderTarget::Top => "Top",
            BorderTarget::Right => "Right",
            BorderTarget::Bottom => "Bottom",
            BorderTarget::Left => "Left",
            BorderTarget::None => "None",
        };
        let json = serde_json::json!({
            "item": { "style": "thin", "color": color },
            "type": type_name,
        });
        let border: ironcalc::base::BorderArea =
            serde_json::from_value(json).map_err(|e| anyhow!("border: {e}"))?;
        self.um
            .set_area_with_border(&range.area(sheet), &border)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    // ----- history -----

    pub fn undo(&mut self) {
        if self.um.undo().is_ok() {
            self.step();
        }
    }

    pub fn redo(&mut self) {
        if self.um.redo().is_ok() {
            self.step();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.um.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.um.can_redo()
    }

    // ----- clipboard (formula aware) -----

    /// Tells the engine which cells are selected; required before copy/paste,
    /// which read the engine's own selection state.
    ///
    /// `set_selected_range` rejects any range whose corner is not the currently
    /// selected cell, so the cell has to be moved to the range's origin first.
    pub fn sync_selection(&mut self, sheet: u32, range: Range) -> Result<()> {
        self.um.set_selected_sheet(sheet).map_err(|e| anyhow!(e))?;
        self.um
            .set_selected_cell(range.r0, range.c0)
            .map_err(|e| anyhow!(e))?;
        self.um
            .set_selected_range(range.r0, range.c0, range.r1, range.c1)
            .map_err(|e| anyhow!(e))?;
        Ok(())
    }

    /// Copies the range into the internal (formula-aware) clipboard and
    /// returns the TSV text for the system clipboard.
    pub fn copy(&mut self, sheet: u32, range: Range) -> Result<String> {
        self.sync_selection(sheet, range)?;
        let clip = self.um.copy_to_clipboard().map_err(|e| anyhow!(e))?;
        let value = serde_json::to_value(&clip).map_err(|e| anyhow!("clipboard: {e}"))?;
        let csv = value
            .get("csv")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let data = value.get("data").cloned().unwrap_or(serde_json::Value::Null);
        self.clipboard = Some(InternalClip {
            sheet,
            range: (range.r0, range.c0, range.r1, range.c1),
            data,
            csv: csv.clone(),
        });
        Ok(csv)
    }

    pub fn has_internal_clip(&self) -> bool {
        self.clipboard.is_some()
    }

    /// True when the system clipboard still holds what we last copied, meaning
    /// the richer internal copy is the right thing to paste.
    pub fn clip_matches(&self, text: &str) -> bool {
        self.clipboard
            .as_ref()
            .map(|c| c.csv.trim() == text.trim())
            .unwrap_or(false)
    }

    /// Pastes the internal clipboard at the target, adjusting relative
    /// references. `is_cut` clears the source.
    pub fn paste_internal(&mut self, sheet: u32, at: (i32, i32), is_cut: bool) -> Result<()> {
        let Some(clip) = &self.clipboard else {
            return Err(anyhow!("nothing copied"));
        };
        let (source_sheet, source_range, data) = (clip.sheet, clip.range, clip.data.clone());
        let data: ironcalc::base::ClipboardData =
            serde_json::from_value(data).map_err(|e| anyhow!("clipboard: {e}"))?;
        self.sync_selection(sheet, Range::single(at.0, at.1))?;
        self.um
            .paste_from_clipboard(source_sheet, source_range, &data, is_cut)
            .map_err(|e| anyhow!(e))?;
        if is_cut {
            self.clipboard = None;
        }
        self.step();
        Ok(())
    }

    /// Pastes plain TSV/CSV text (from another application).
    pub fn paste_text(&mut self, sheet: u32, at: (i32, i32), text: &str) -> Result<()> {
        let rows = text.lines().count().max(1) as i32;
        let cols = text
            .lines()
            .map(|l| l.split('\t').count())
            .max()
            .unwrap_or(1) as i32;
        let area = Area {
            sheet,
            row: at.0,
            column: at.1,
            width: cols,
            height: rows,
        };
        self.um
            .paste_csv_string(&area, text)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    // ----- structure -----

    pub fn insert_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<()> {
        self.um
            .insert_rows(sheet, row, count)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn delete_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<()> {
        self.um
            .delete_rows(sheet, row, count)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn insert_cols(&mut self, sheet: u32, col: i32, count: i32) -> Result<()> {
        self.um
            .insert_columns(sheet, col, count)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn delete_cols(&mut self, sheet: u32, col: i32, count: i32) -> Result<()> {
        self.um
            .delete_columns(sheet, col, count)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn set_col_width(&mut self, sheet: u32, c0: i32, c1: i32, width: f64) -> Result<()> {
        self.um
            .set_columns_width(sheet, c0, c1, width.max(0.0))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn set_row_height(&mut self, sheet: u32, r0: i32, r1: i32, height: f64) -> Result<()> {
        self.um
            .set_rows_height(sheet, r0, r1, height.max(0.0))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn set_rows_hidden(&mut self, sheet: u32, r0: i32, r1: i32, hidden: bool) -> Result<()> {
        self.um
            .set_rows_hidden(sheet, r0, r1, hidden)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn is_row_hidden(&self, sheet: u32, row: i32) -> bool {
        self.um
            .get_model()
            .workbook
            .worksheet(sheet)
            .and_then(|ws| ws.is_row_hidden(row))
            .unwrap_or(false)
    }

    pub fn col_width(&self, sheet: u32, col: i32) -> f32 {
        self.um
            .get_column_width(sheet, col)
            .map(|w| w as f32)
            .unwrap_or(100.0)
            .max(0.0)
    }

    pub fn row_height(&self, sheet: u32, row: i32) -> f32 {
        if self.is_row_hidden(sheet, row) {
            return 0.0;
        }
        self.um
            .get_row_height(sheet, row)
            .map(|h| h as f32)
            .unwrap_or(21.0)
            .max(0.0)
    }

    /// Width that fits the widest value in the column, in IronCalc units.
    pub fn autofit_width(&self, sheet: u32, col: i32) -> f64 {
        let (last_row, _) = self.used_range(sheet);
        let mut widest = 0usize;
        for row in 1..=last_row.min(2000) {
            widest = widest.max(self.display(sheet, row, col).chars().count());
        }
        ((widest as f64 * 7.5) + 12.0).clamp(32.0, 800.0)
    }

    pub fn frozen(&self, sheet: u32) -> (i32, i32) {
        (
            self.um.get_frozen_rows_count(sheet).unwrap_or(0),
            self.um.get_frozen_columns_count(sheet).unwrap_or(0),
        )
    }

    pub fn set_frozen(&mut self, sheet: u32, rows: i32, cols: i32) -> Result<()> {
        self.um
            .set_frozen_rows_count(sheet, rows.max(0))
            .map_err(|e| anyhow!(e))?;
        self.um
            .set_frozen_columns_count(sheet, cols.max(0))
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    /// Merged regions on the sheet, as (r0, r1, c0, c1).
    pub fn merges(&self, sheet: u32) -> Vec<Range> {
        let Ok(ws) = self.um.get_model().workbook.worksheet(sheet) else {
            return Vec::new();
        };
        ws.merge_cells
            .iter()
            .filter_map(|s| parse_a1_range(s))
            .collect()
    }

    // ----- sheets -----

    /// Index of a sheet by name, case-insensitive.
    pub fn sheet_index(&self, name: &str) -> Option<u32> {
        self.sheet_names()
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| i as u32)
    }

    /// Displayed values for a whole block, row-major. One call instead of
    /// one per cell, which matters when a tool reads a window of the sheet.
    pub fn read_range(&self, sheet: u32, range: Range) -> Vec<Vec<String>> {
        (range.r0..=range.r1)
            .map(|r| {
                (range.c0..=range.c1)
                    .map(|c| self.display(sheet, r, c))
                    .collect()
            })
            .collect()
    }

    /// Raw cell contents (formulas as typed) for a whole block, row-major.
    pub fn read_range_content(&self, sheet: u32, range: Range) -> Vec<Vec<String>> {
        (range.r0..=range.r1)
            .map(|r| {
                (range.c0..=range.c1)
                    .map(|c| self.content(sheet, r, c))
                    .collect()
            })
            .collect()
    }

    /// Batches a run of edits: evaluation is paused until `end_transaction`,
    /// so a tool writing 50 cells recalculates once rather than 50 times.
    ///
    /// This does *not* collapse the undo history — IronCalc keeps `history`
    /// private with no public grouping API — so the app counts the steps a
    /// batch produced and undoes that many at once (see `undo_groups` in
    /// `GridoApp`).
    pub fn begin_transaction(&mut self) {
        self.um.pause_evaluation();
        self.undo_steps = 0;
    }

    /// Ends the batch, recalculates, and returns how many undo entries the
    /// batch produced.
    pub fn end_transaction(&mut self) -> usize {
        self.um.resume_evaluation();
        self.um.evaluate();
        std::mem::take(&mut self.undo_steps)
    }

    /// Counts one undo entry. Called by every mutating wrapper so the app can
    /// group a tool call's edits into a single Ctrl+Z.
    fn step(&mut self) {
        self.undo_steps = self.undo_steps.saturating_add(1);
        self.dirty = true;
    }

    pub fn sheet_names(&self) -> Vec<String> {
        self.um.get_model().workbook.get_worksheet_names()
    }

    pub fn sheet_color(&self, sheet: u32) -> Option<[u8; 3]> {
        let props = self.um.get_worksheets_properties();
        let p = props.get(sheet as usize)?;
        self.resolve(&p.color)
    }

    pub fn new_sheet(&mut self) -> Result<()> {
        self.um.new_sheet().map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn delete_sheet(&mut self, sheet: u32) -> Result<()> {
        self.um.delete_sheet(sheet).map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn rename_sheet(&mut self, sheet: u32, name: &str) -> Result<()> {
        self.um.rename_sheet(sheet, name).map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn duplicate_sheet(&mut self, sheet: u32) -> Result<()> {
        self.um.duplicate_sheet(sheet).map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn move_sheet(&mut self, sheet: u32, to: u32) -> Result<()> {
        self.um.move_sheet(sheet, to).map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn set_sheet_color(&mut self, sheet: u32, color: &str) -> Result<()> {
        let c = if color.is_empty() {
            Color::None
        } else {
            Color::from_param(color).map_err(|e| anyhow!(e))?
        };
        self.um.set_sheet_color(sheet, &c).map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn show_grid_lines(&self, sheet: u32) -> bool {
        self.um.get_show_grid_lines(sheet).unwrap_or(true)
    }

    pub fn set_show_grid_lines(&mut self, sheet: u32, show: bool) -> Result<()> {
        self.um
            .set_show_grid_lines(sheet, show)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    /// (last used row, last used column) of the sheet, at least (1, 1).
    pub fn used_range(&self, sheet: u32) -> (i32, i32) {
        self.um
            .get_model()
            .workbook
            .worksheet(sheet)
            .map(|ws| {
                let d = ws.dimension();
                (d.max_row.max(1), d.max_column.max(1))
            })
            .unwrap_or((1, 1))
    }

    // ----- fill -----

    /// Series fill: extends the top row of `src` down to `to_row`,
    /// adjusting relative references.
    pub fn auto_fill_rows(&mut self, sheet: u32, src: Range, to_row: i32) -> Result<()> {
        self.um
            .auto_fill_rows(&src.area(sheet), to_row)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn auto_fill_columns(&mut self, sheet: u32, src: Range, to_col: i32) -> Result<()> {
        self.um
            .auto_fill_columns(&src.area(sheet), to_col)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    // ----- formulas -----

    /// Function-name suggestions for a formula being typed.
    pub fn completions(&mut self, sheet: u32, row: i32, col: i32, text: &str) -> Vec<String> {
        let cursor = text.chars().count();
        let Ok(ctx) = self.um.formula_completion(sheet, row, col, text, cursor) else {
            return Vec::new();
        };
        let Ok(value) = serde_json::to_value(&ctx) else {
            return Vec::new();
        };
        // The completion context shape varies by version; pull out any string
        // list of candidate names we can find.
        fn collect(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Array(items) => {
                    for item in items {
                        collect(item, out);
                    }
                }
                serde_json::Value::Object(map) => {
                    for (key, val) in map {
                        if key == "name" || key == "text" || key == "label" {
                            if let Some(s) = val.as_str() {
                                out.push(s.to_string());
                                continue;
                            }
                        }
                        collect(val, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        collect(&value, &mut out);
        out.sort();
        out.dedup();
        out.truncate(12);
        out
    }

    /// F4: cycle the reference under the cursor through its $ variants.
    pub fn cycle_reference(&self, text: &str, start: usize, end: usize) -> Option<(String, usize)> {
        let (new_text, _s, e) = self.um.cycle_reference(text, start, end).ok()?;
        Some((new_text, e.max(0) as usize))
    }

    // ----- defined names -----

    pub fn defined_names(&self) -> Vec<(String, String)> {
        self.um
            .get_defined_name_list()
            .into_iter()
            .map(|(name, _scope, formula)| (name, formula))
            .collect()
    }

    /// Adds a workbook-scoped name. The reference may be written with or
    /// without a leading `=` — IronCalc lexes the bare reference, but users
    /// type the `=` form, so strip it here.
    pub fn add_defined_name(&mut self, name: &str, formula: &str) -> Result<()> {
        let reference = formula.trim().strip_prefix('=').unwrap_or(formula.trim());
        self.um
            .new_defined_name(name, None, reference)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    pub fn delete_defined_name(&mut self, name: &str) -> Result<()> {
        self.um
            .delete_defined_name(name, None)
            .map_err(|e| anyhow!(e))?;
        self.step();
        Ok(())
    }

    // ----- find & replace -----

    pub fn find(
        &self,
        sheet: u32,
        needle: &str,
        match_case: bool,
        whole_workbook: bool,
    ) -> Vec<FindHit> {
        let mut hits = Vec::new();
        let sheets: Vec<u32> = if whole_workbook {
            (0..self.sheet_names().len() as u32).collect()
        } else {
            vec![sheet]
        };
        let needle_cmp = if match_case {
            needle.to_string()
        } else {
            needle.to_lowercase()
        };
        for s in sheets {
            let (max_row, max_col) = self.used_range(s);
            for row in 1..=max_row {
                for col in 1..=max_col {
                    let text = self.display(s, row, col);
                    if text.is_empty() {
                        continue;
                    }
                    let hay = if match_case {
                        text
                    } else {
                        text.to_lowercase()
                    };
                    if hay.contains(&needle_cmp) {
                        hits.push(FindHit {
                            sheet: s,
                            row,
                            col,
                        });
                    }
                }
            }
        }
        hits
    }

    /// Replaces in raw cell content. Returns how many cells changed.
    pub fn replace_all(
        &mut self,
        sheet: u32,
        needle: &str,
        replacement: &str,
        match_case: bool,
        whole_workbook: bool,
    ) -> usize {
        if needle.is_empty() {
            return 0;
        }
        let hits = self.find(sheet, needle, match_case, whole_workbook);
        let mut changed = 0;
        for hit in hits {
            let content = self.content(hit.sheet, hit.row, hit.col);
            let new = if match_case {
                content.replace(needle, replacement)
            } else {
                replace_ignore_case(&content, needle, replacement)
            };
            if new != content && self.set_input(hit.sheet, hit.row, hit.col, &new).is_ok() {
                changed += 1;
            }
        }
        changed
    }

    // ----- sort -----

    /// Sorts `range` by `key_col` (absolute column index). Moves raw cell
    /// content; formulas move verbatim (references are not rewritten).
    pub fn sort_range(
        &mut self,
        sheet: u32,
        range: Range,
        key_col: i32,
        ascending: bool,
    ) -> Result<()> {
        self.sort_range_by(sheet, range, &[(key_col, ascending)])
    }

    /// Multi-key sort: `keys` are listed most-significant first. Rows are read
    /// once, ordered in memory and written back once, so cost does not grow
    /// with the number of keys.
    pub fn sort_range_by(
        &mut self,
        sheet: u32,
        range: Range,
        keys: &[(i32, bool)],
    ) -> Result<()> {
        if keys.is_empty() || range.rows() < 2 {
            return Ok(());
        }
        // Each row: its sort keys (as displayed) plus every raw cell value.
        let mut rows: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        for r in range.r0..=range.r1 {
            let sort_keys = keys
                .iter()
                .map(|(col, _)| self.display(sheet, r, *col))
                .collect();
            let cells = (range.c0..=range.c1)
                .map(|c| self.content(sheet, r, c))
                .collect();
            rows.push((sort_keys, cells));
        }
        rows.sort_by(|a, b| {
            for (i, (_, ascending)) in keys.iter().enumerate() {
                let cmp = compare_cells(&a.0[i], &b.0[i]);
                let cmp = if *ascending { cmp } else { cmp.reverse() };
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        });
        for (i, (_, cells)) in rows.iter().enumerate() {
            let r = range.r0 + i as i32;
            for (j, value) in cells.iter().enumerate() {
                let c = range.c0 + j as i32;
                self.set_input(sheet, r, c, value)?;
            }
        }
        Ok(())
    }

    /// Distinct displayed values in a column over a row span, for filter UIs.
    pub fn distinct_values(&self, sheet: u32, col: i32, r0: i32, r1: i32) -> Vec<String> {
        let mut values: Vec<String> = Vec::new();
        for r in r0..=r1 {
            let v = self.display(sheet, r, col);
            if !values.contains(&v) {
                values.push(v);
            }
        }
        values.sort();
        values
    }

    // ----- statistics for the status bar -----

    /// (count of non-empty, count of numeric, sum, average) over a range.
    pub fn stats(&self, sheet: u32, range: Range) -> (usize, usize, f64, f64) {
        let mut count = 0;
        let mut numeric = 0;
        let mut sum = 0.0;
        for r in range.r0..=range.r1 {
            for c in range.c0..=range.c1 {
                if self.is_empty(sheet, r, c) {
                    continue;
                }
                count += 1;
                if self.is_number(sheet, r, c) {
                    if let Ok(v) = self.display(sheet, r, c).replace(',', "").parse::<f64>() {
                        numeric += 1;
                        sum += v;
                    }
                }
            }
        }
        let avg = if numeric > 0 {
            sum / numeric as f64
        } else {
            0.0
        };
        (count, numeric, sum, avg)
    }
}

/// Orders two displayed cell values: numbers numerically, blanks last, the
/// rest case-insensitively — the usual spreadsheet ordering.
fn compare_cells(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// True when a style already holds `value` at `path`, so writing is pointless.
fn style_value_matches(style: &ironcalc::base::types::Style, path: &str, value: &str) -> bool {
    let current = match path {
        "font.b" => style.font.b.to_string(),
        "font.i" => style.font.i.to_string(),
        "font.u" => style.font.u.to_string(),
        "font.strike" => style.font.strike.to_string(),
        "font.size" => style.font.sz.to_string(),
        "num_fmt" => style.num_fmt.clone(),
        "font.color" => color_param(&style.font.color),
        "fill.color" => color_param(&style.fill.color),
        _ => return false,
    };
    current.eq_ignore_ascii_case(value)
}

fn color_param(color: &Color) -> String {
    match color {
        Color::Rgb(s) => s.clone(),
        _ => String::new(),
    }
}

fn replace_ignore_case(haystack: &str, needle: &str, replacement: &str) -> String {
    let lower_hay = haystack.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let mut out = String::new();
    let mut i = 0;
    while let Some(pos) = lower_hay[i..].find(&lower_needle) {
        let start = i + pos;
        out.push_str(&haystack[i..start]);
        out.push_str(replacement);
        i = start + needle.len();
    }
    out.push_str(&haystack[i..]);
    out
}

/// Renders a Range as an A1 reference like "B2:D9".
pub fn a1_range(range: Range) -> String {
    format!(
        "{}{}:{}{}",
        col_letters(range.c0),
        range.r0,
        col_letters(range.c1),
        range.r1
    )
}

/// A1-style column letters for a 1-based column index.
pub fn col_letters(mut col: i32) -> String {
    let mut name = String::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        name.insert(0, (b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    name
}

/// Parses "A1:B2" (or "A1") into a Range.
pub fn parse_a1_range(s: &str) -> Option<Range> {
    let (a, b) = match s.split_once(':') {
        Some((a, b)) => (a, b),
        None => (s, s),
    };
    let (r0, c0) = parse_a1(a)?;
    let (r1, c1) = parse_a1(b)?;
    Some(Range {
        r0: r0.min(r1),
        r1: r0.max(r1),
        c0: c0.min(c1),
        c1: c0.max(c1),
    })
}

fn parse_a1(s: &str) -> Option<(i32, i32)> {
    let s = s.replace('$', "");
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = s.split_at(split);
    if letters.is_empty() {
        return None;
    }
    let mut col = 0i32;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (ch.to_ascii_uppercase() as i32 - 'A' as i32 + 1);
    }
    let row: i32 = digits.parse().ok()?;
    Some((row, col))
}

/// Parses a cell reference like "B12" typed into the Name Box.
pub fn parse_cell_ref(s: &str) -> Option<(i32, i32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (row, col) = parse_a1(s)?;
    if row < 1 || row > MAX_ROWS || col < 1 || col > MAX_COLS {
        return None;
    }
    Some((row, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a1_parsing() {
        assert_eq!(parse_a1("A1"), Some((1, 1)));
        assert_eq!(parse_a1("B12"), Some((12, 2)));
        assert_eq!(parse_a1("AA3"), Some((3, 27)));
        assert_eq!(parse_a1("$C$7"), Some((7, 3)));
        assert_eq!(parse_a1("nonsense"), None);
    }

    #[test]
    fn a1_range_parsing() {
        let r = parse_a1_range("A1:B3").unwrap();
        assert_eq!((r.r0, r.r1, r.c0, r.c1), (1, 3, 1, 2));
        let single = parse_a1_range("C2").unwrap();
        assert_eq!((single.r0, single.r1, single.c0, single.c1), (2, 2, 3, 3));
    }

    #[test]
    fn case_insensitive_replace() {
        assert_eq!(replace_ignore_case("Hello hello", "hello", "hi"), "hi hi");
        assert_eq!(replace_ignore_case("abc", "x", "y"), "abc");
    }

    #[test]
    fn hex_colors() {
        assert_eq!(hex_to_rgb("#FF8800"), Some([255, 136, 0]));
        assert_eq!(hex_to_rgb("bad"), None);
    }
}
