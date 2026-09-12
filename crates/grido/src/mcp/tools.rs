//! Tool definitions and the dispatch that runs them against the live workbook.
//!
//! Two levels on purpose. Cell-level tools (`grido_cell_set`,
//! `grido_range_read`) are the primitives; record-level tools
//! (`grido_table_*`) treat a header row as a schema so an assistant can say
//! "the Acme row" instead of computing that Acme is on row 14 — which is where
//! models make mistakes.
//!
//! Every tool runs on the UI thread, so `&mut GridoApp` is available and the
//! user sees each change as it happens.
//!
//! The names carry the `grido_` prefix because one MCP server speaks for the
//! whole product: the spreadsheet's verbs sit next to the enclave's own.

use anyhow::{anyhow, bail, Context as _, Result};
use serde_json::{json, Value};

use crate::engine::{parse_a1_range, Range, StyleEdit};
use crate::state::GridoApp;

/// Never hand back more than this many cells in one read: a naive
/// "show me the sheet" must not blow up the model's context.
const MAX_READ_CELLS: usize = 5_000;

/// What every tool here is called. The host dispatches on it, and only a call
/// carrying it is worth opening the window for.
pub const PREFIX: &str = "grido_";

pub fn definitions() -> Vec<Value> {
    let tool = |name: &str, desc: &str, props: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": desc,
            "inputSchema": {
                "type": "object",
                "properties": props,
                "required": required,
            }
        })
    };
    let sheet = json!({
        "type": "string",
        "description": "sheet name; defaults to the one on screen"
    });
    vec![
        tool(
            "grido_workbook_info",
            "The open workbook: file path, unsaved-changes flag, and every sheet \
             with its used range and header row. Call this first — it tells you \
             what exists before you read or write anything.",
            json!({}),
            &[],
        ),
        tool(
            "grido_workbook_open",
            "Open an .xlsx file in the Grido window, replacing what is open.",
            json!({ "path": {"type": "string"} }),
            &["path"],
        ),
        tool(
            "grido_workbook_save",
            "Save the workbook. Without a path it saves over the file it came \
             from; a new workbook needs a path.",
            json!({ "path": {"type": "string", "description": "optional target path"} }),
            &[],
        ),
        tool(
            "grido_range_read",
            "Displayed values for a range, as rows. Omit `range` to read the \
             used part of the sheet. Set `formulas` to see what was typed \
             rather than the computed result.",
            json!({
                "sheet": sheet,
                "range": {"type": "string", "description": "A1 notation, e.g. \"A1:D20\""},
                "formulas": {"type": "boolean", "default": false},
            }),
            &[],
        ),
        tool(
            "grido_cell_set",
            "Write one cell. A value starting with = is a formula and is \
             evaluated; everything else is text, a number or a date.",
            json!({
                "sheet": sheet,
                "cell": {"type": "string", "description": "A1 notation, e.g. \"B7\""},
                "value": {"type": "string"},
            }),
            &["cell", "value"],
        ),
        tool(
            "grido_range_set",
            "Write a block of cells in one step, given as rows of values. The \
             block is anchored at the top-left of `range`.",
            json!({
                "sheet": sheet,
                "range": {"type": "string", "description": "anchor cell or full range"},
                "values": {
                    "type": "array",
                    "description": "rows, each an array of cell values",
                    "items": {"type": "array", "items": {"type": "string"}},
                },
            }),
            &["range", "values"],
        ),
        tool(
            "grido_range_format",
            "Formatting for a range: bold/italic, colors, alignment, and the \
             number format (currency, percent, date …).",
            json!({
                "sheet": sheet,
                "range": {"type": "string"},
                "bold": {"type": "boolean"},
                "italic": {"type": "boolean"},
                "font_color": {"type": "string", "description": "#RRGGBB"},
                "fill_color": {"type": "string", "description": "#RRGGBB"},
                "align": {"type": "string", "enum": ["left", "center", "right"]},
                "number_format": {
                    "type": "string",
                    "description": "one of general/currency/percent/comma/date, or a raw format code"
                },
            }),
            &["range"],
        ),
        tool(
            "grido_table_schema",
            "The header row of a sheet and how many data rows follow it. Use \
             this before the other grido_table_ tools.",
            json!({ "sheet": sheet }),
            &[],
        ),
        tool(
            "grido_table_append",
            "Add a record to the end of a table, given as column-name → value. \
             Columns are matched against the header row, so you never compute \
             row or column numbers.",
            json!({
                "sheet": sheet,
                "record": {"type": "object", "description": "column name → value"},
            }),
            &["record"],
        ),
        tool(
            "grido_table_find",
            "Rows whose columns match every criterion (case-insensitive). \
             Returns each match with its row number and full record.",
            json!({
                "sheet": sheet,
                "criteria": {"type": "object", "description": "column name → value to match"},
            }),
            &["criteria"],
        ),
        tool(
            "grido_table_update",
            "Update the rows matching `criteria`, setting the columns in \
             `changes`. Refuses ambiguous updates unless `all` is true, so a \
             typo cannot rewrite the whole table.",
            json!({
                "sheet": sheet,
                "criteria": {"type": "object"},
                "changes": {"type": "object"},
                "all": {"type": "boolean", "default": false,
                        "description": "allow updating more than one matching row"},
            }),
            &["criteria", "changes"],
        ),
        tool(
            "grido_rows_insert",
            "Insert blank rows; formulas referring to shifted cells are fixed up.",
            json!({
                "sheet": sheet,
                "at": {"type": "integer", "description": "1-based row to insert before"},
                "count": {"type": "integer", "default": 1},
            }),
            &["at"],
        ),
        tool(
            "grido_rows_delete",
            "Delete rows. Destructive: the user is asked to confirm in the app.",
            json!({
                "sheet": sheet,
                "at": {"type": "integer"},
                "count": {"type": "integer", "default": 1},
            }),
            &["at"],
        ),
        tool(
            "grido_sheet_add",
            "Add a new empty sheet to the workbook, optionally with a name. \
             Use this to keep separate concerns apart rather than crowding \
             one sheet.",
            json!({ "name": {"type": "string"} }),
            &[],
        ),
        tool(
            "grido_sheet_rename",
            "Rename a sheet. Formulas that reference it by name are updated to \
             match, so this is safe to do on a sheet already in use.",
            json!({ "sheet": sheet, "name": {"type": "string"} }),
            &["name"],
        ),
        tool(
            "grido_sort",
            "Sort a range by one of its columns.",
            json!({
                "sheet": sheet,
                "range": {"type": "string"},
                "by": {"type": "string", "description": "column letter, or a header name"},
                "descending": {"type": "boolean", "default": false},
                "headers": {"type": "boolean", "default": true,
                            "description": "treat the first row as headers and keep it in place"},
            }),
            &["range", "by"],
        ),
        tool(
            "grido_find",
            "Search the sheet for text and return the cells that matched.",
            json!({
                "sheet": sheet,
                "text": {"type": "string"},
                "match_case": {"type": "boolean", "default": false},
            }),
            &["text"],
        ),
        tool(
            "grido_stats",
            "Sum, average, count and min/max of the numbers in a range.",
            json!({ "sheet": sheet, "range": {"type": "string"} }),
            &["range"],
        ),
        tool(
            "grido_template_create",
            "Create a ready-made workbook: headers, formats, a frozen header \
             row and summary formulas. Fastest way to start something useful.",
            json!({
                "kind": {"type": "string", "enum": ["job-applications", "budget", "contacts"]},
                "path": {"type": "string", "description": "optional path to save it to"},
            }),
            &["kind"],
        ),
        tool(
            "grido_undo",
            "Undo the last action, including a whole assistant edit.",
            json!({}),
            &[],
        ),
    ]
}

// ----- helpers -------------------------------------------------------------

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing \"{key}\""))
}

fn sheet_of(app: &GridoApp, args: &Value) -> Result<u32> {
    match args.get("sheet").and_then(Value::as_str) {
        Some(name) => app.engine.sheet_index(name).ok_or_else(|| {
            anyhow!(
                "no sheet named \"{name}\" (have: {})",
                app.engine.sheet_names().join(", ")
            )
        }),
        None => Ok(app.sheet),
    }
}

fn range_of(args: &Value, key: &str) -> Result<Range> {
    let text = str_arg(args, key)?;
    parse_a1_range(text).ok_or_else(|| anyhow!("\"{text}\" is not a range like A1 or A1:D20"))
}

/// The used block of a sheet, capped so a read can't be unbounded.
fn used(app: &GridoApp, sheet: u32) -> Range {
    let (r, c) = app.engine.used_range(sheet);
    Range {
        r0: 1,
        r1: r.max(1),
        c0: 1,
        c1: c.max(1),
    }
}

fn check_size(range: Range) -> Result<()> {
    let cells = range.rows() as usize * range.cols() as usize;
    if cells > MAX_READ_CELLS {
        bail!(
            "that range is {cells} cells; read at most {MAX_READ_CELLS} at a time \
             (ask for a smaller range)"
        );
    }
    Ok(())
}

fn col_letters(mut col: i32) -> String {
    let mut out = String::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        out.insert(0, (b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    out
}

/// A table found on a sheet: where its header row is, which columns belong to
/// it, and where its data ends.
///
/// The column span is the *contiguous* run of non-empty header cells. That
/// matters: a sheet often has other things off to the side (summary formulas,
/// notes), and those must not be mistaken for table columns — otherwise a row
/// holding only a summary label looks occupied and appends land in the wrong
/// place.
pub struct Table {
    pub header_row: i32,
    pub c0: i32,
    pub headers: Vec<String>,
    /// Last row containing data in the table's own columns.
    pub last_row: i32,
}

impl Table {
    fn c1(&self) -> i32 {
        self.c0 + self.headers.len() as i32 - 1
    }

    fn range(&self) -> Range {
        Range {
            r0: self.header_row,
            r1: self.last_row.max(self.header_row),
            c0: self.c0,
            c1: self.c1(),
        }
    }
}

fn table_of(app: &GridoApp, sheet: u32) -> Result<Table> {
    let block = used(app, sheet);
    for r in block.r0..=block.r1.min(block.r0 + 20) {
        // First non-empty cell on this row starts the header.
        let Some(c0) = (block.c0..=block.c1).find(|c| !app.engine.is_empty(sheet, r, *c)) else {
            continue;
        };
        // ...and the header runs until the first gap.
        let mut headers = Vec::new();
        let mut c = c0;
        while c <= block.c1 && !app.engine.is_empty(sheet, r, c) {
            headers.push(app.engine.display(sheet, r, c));
            c += 1;
        }
        let c1 = c0 + headers.len() as i32 - 1;
        let last_row = (r..=block.r1)
            .filter(|row| (c0..=c1).any(|col| !app.engine.is_empty(sheet, *row, col)))
            .next_back()
            .unwrap_or(r);
        return Ok(Table {
            header_row: r,
            c0,
            headers,
            last_row,
        });
    }
    bail!("this sheet is empty — no header row to match columns against")
}

fn column_of(headers: &[String], name: &str, first_col: i32) -> Result<i32> {
    headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
        .map(|i| first_col + i as i32)
        .ok_or_else(|| {
            anyhow!(
                "no column named \"{name}\" (have: {})",
                headers
                    .iter()
                    .filter(|h| !h.trim().is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn record_of(app: &GridoApp, sheet: u32, row: i32, headers: &[String], c0: i32) -> Value {
    let mut map = serde_json::Map::new();
    for (i, name) in headers.iter().enumerate() {
        if name.trim().is_empty() {
            continue;
        }
        let value = app.engine.display(sheet, row, c0 + i as i32);
        map.insert(name.clone(), Value::String(value));
    }
    Value::Object(map)
}

fn num_fmt_code(spec: &str) -> String {
    match spec.to_ascii_lowercase().as_str() {
        "general" => "general".to_string(),
        "currency" => "\"$\"#,##0.00".to_string(),
        "percent" => "0.0%".to_string(),
        "comma" => "#,##0.00".to_string(),
        "date" => "yyyy-mm-dd".to_string(),
        _ => spec.to_string(), // a raw format code
    }
}

// ----- dispatch ------------------------------------------------------------

pub fn dispatch(app: &mut GridoApp, tool: &str, args: &Value) -> Result<Value> {
    match tool {
        "grido_workbook_info" => workbook_info(app),
        "grido_workbook_open" => workbook_open(app, args),
        "grido_workbook_save" => workbook_save(app, args),
        "grido_range_read" => range_read(app, args),
        "grido_cell_set" => cell_set(app, args),
        "grido_range_set" => range_set(app, args),
        "grido_range_format" => range_format(app, args),
        "grido_table_schema" => table_schema(app, args),
        "grido_table_append" => table_append(app, args),
        "grido_table_find" => table_find(app, args),
        "grido_table_update" => table_update(app, args),
        "grido_rows_insert" => rows_insert(app, args),
        "grido_rows_delete" => rows_delete(app, args),
        "grido_sheet_add" => sheet_add(app, args),
        "grido_sheet_rename" => sheet_rename(app, args),
        "grido_sort" => sort(app, args),
        "grido_find" => find(app, args),
        "grido_stats" => stats(app, args),
        "grido_template_create" => crate::mcp::templates::create(app, args),
        "grido_undo" => {
            app.undo_grouped();
            Ok(json!("undone"))
        }
        other => bail!("no such tool: {other}"),
    }
}

fn workbook_info(app: &mut GridoApp) -> Result<Value> {
    let sheets: Vec<Value> = app
        .engine
        .sheet_names()
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let s = i as u32;
            let (rows, cols) = app.engine.used_range(s);
            let headers = table_of(app, s).map(|t| t.headers).unwrap_or_default();
            json!({
                "name": name,
                "rows": rows,
                "columns": cols,
                "used_range": format!("A1:{}{}", col_letters(cols.max(1)), rows.max(1)),
                "headers": headers.iter().filter(|h| !h.trim().is_empty()).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "path": app.engine.path.as_ref().map(|p| p.display().to_string()),
        "unsaved_changes": app.engine.dirty,
        "active_sheet": app.engine.sheet_names().get(app.sheet as usize),
        "sheets": sheets,
    }))
}

fn workbook_open(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let path = std::path::PathBuf::from(str_arg(args, "path")?);
    if !path.is_file() {
        bail!("no such file: {}", path.display());
    }
    if app.engine.dirty {
        bail!(
            "the open workbook has unsaved changes — call grido_workbook_save \
             first, or ask the user to save"
        );
    }
    app.open_path(path.clone());
    Ok(json!(format!("opened {}", path.display())))
}

fn workbook_save(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let target = match args.get("path").and_then(Value::as_str) {
        Some(p) => Some(std::path::PathBuf::from(p)),
        None => app.engine.path.clone(),
    };
    let path = target.context("this workbook has no path yet — pass one")?;
    app.engine.save(&path)?;
    Ok(json!(format!("saved {}", path.display())))
}

fn range_read(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let range = match args.get("range").and_then(Value::as_str) {
        Some(text) => {
            parse_a1_range(text).ok_or_else(|| anyhow!("\"{text}\" is not a range"))?
        }
        None => used(app, sheet),
    };
    check_size(range)?;
    let formulas = args
        .get("formulas")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rows = if formulas {
        app.engine.read_range_content(sheet, range)
    } else {
        app.engine.read_range(sheet, range)
    };
    Ok(json!({
        "sheet": app.engine.sheet_names()[sheet as usize],
        "range": crate::engine::a1_range(range),
        "rows": rows,
    }))
}

fn cell_set(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let cell = range_of(args, "cell")?;
    let value = str_arg(args, "value")?;
    app.engine.set_input(sheet, cell.r0, cell.c0, value)?;
    app.note_change(sheet, cell);
    Ok(json!({
        "cell": crate::engine::a1_range(cell),
        "value": app.engine.display(sheet, cell.r0, cell.c0),
    }))
}

fn range_set(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let anchor = range_of(args, "range")?;
    let rows = args
        .get("values")
        .and_then(Value::as_array)
        .context("missing \"values\" (an array of rows)")?;

    app.engine.begin_transaction();
    let mut written = 0usize;
    let mut last = anchor;
    for (dr, row) in rows.iter().enumerate() {
        let cells = row.as_array().context("each row must be an array")?;
        for (dc, cell) in cells.iter().enumerate() {
            let r = anchor.r0 + dr as i32;
            let c = anchor.c0 + dc as i32;
            let text = match cell {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            app.engine.set_input(sheet, r, c, &text)?;
            written += 1;
            last = Range {
                r0: anchor.r0,
                r1: r,
                c0: anchor.c0,
                c1: c.max(last.c1),
            };
        }
    }
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    app.note_change(sheet, last);
    Ok(json!({ "written": written, "range": crate::engine::a1_range(last) }))
}

fn range_format(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let range = range_of(args, "range")?;
    let mut edits = Vec::new();
    if let Some(v) = args.get("bold").and_then(Value::as_bool) {
        edits.push(StyleEdit::Bold(v));
    }
    if let Some(v) = args.get("italic").and_then(Value::as_bool) {
        edits.push(StyleEdit::Italic(v));
    }
    if let Some(v) = args.get("font_color").and_then(Value::as_str) {
        edits.push(StyleEdit::FontColor(Some(v.to_string())));
    }
    if let Some(v) = args.get("fill_color").and_then(Value::as_str) {
        edits.push(StyleEdit::FillColor(Some(v.to_string())));
    }
    if let Some(v) = args.get("align").and_then(Value::as_str) {
        use crate::engine::HAlign;
        edits.push(StyleEdit::Align(match v.to_ascii_lowercase().as_str() {
            "left" => HAlign::Left,
            "center" => HAlign::Center,
            "right" => HAlign::Right,
            other => bail!("align must be left, center or right (got \"{other}\")"),
        }));
    }
    if let Some(v) = args.get("number_format").and_then(Value::as_str) {
        edits.push(StyleEdit::NumFmt(num_fmt_code(v)));
    }
    if edits.is_empty() {
        bail!("nothing to change — pass at least one formatting option");
    }
    app.engine.begin_transaction();
    app.engine.set_styles(sheet, range, &edits)?;
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    app.note_change(sheet, range);
    app.invalidate_geometry();
    Ok(json!(format!(
        "formatted {}",
        crate::engine::a1_range(range)
    )))
}

fn table_schema(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let table = table_of(app, sheet)?;
    Ok(json!({
        "header_row": table.header_row,
        "first_data_row": table.header_row + 1,
        "data_rows": (table.last_row - table.header_row).max(0),
        "range": crate::engine::a1_range(table.range()),
        "columns": table.headers.iter().enumerate()
            .map(|(i, h)| json!({"name": h, "column": col_letters(table.c0 + i as i32)}))
            .collect::<Vec<_>>(),
    }))
}

fn table_append(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let table = table_of(app, sheet)?;
    let record = args
        .get("record")
        .and_then(Value::as_object)
        .context("missing \"record\" (an object of column → value)")?;

    // First row with nothing in the table's own columns; gaps are reused.
    let mut row = table.last_row + 1;
    for r in (table.header_row + 1)..=table.last_row {
        if (table.c0..=table.c1()).all(|c| app.engine.is_empty(sheet, r, c)) {
            row = r;
            break;
        }
    }

    app.engine.begin_transaction();
    for (name, value) in record {
        let col = column_of(&table.headers, name, table.c0)?;
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        app.engine.set_input(sheet, row, col, &text)?;
    }
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    let touched = Range { r0: row, r1: row, c0: table.c0, c1: table.c1() };
    app.note_change(sheet, touched);
    Ok(json!({
        "row": row,
        "record": record_of(app, sheet, row, &table.headers, table.c0),
    }))
}

/// Rows matching every criterion. Shared by the find and update tools.
fn matching_rows(
    app: &GridoApp,
    sheet: u32,
    table: &Table,
    criteria: &serde_json::Map<String, Value>,
) -> Result<Vec<i32>> {
    let mut cols = Vec::new();
    for (name, want) in criteria {
        let col = column_of(&table.headers, name, table.c0)?;
        let want = match want {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        cols.push((col, want));
    }
    Ok(((table.header_row + 1)..=table.last_row)
        .filter(|r| {
            cols.iter().all(|(col, want)| {
                app.engine
                    .display(sheet, *r, *col)
                    .trim()
                    .eq_ignore_ascii_case(want.trim())
            })
        })
        .collect())
}

fn table_find(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let table = table_of(app, sheet)?;
    let criteria = args
        .get("criteria")
        .and_then(Value::as_object)
        .context("missing \"criteria\"")?;
    let rows = matching_rows(app, sheet, &table, criteria)?;
    Ok(json!({
        "matches": rows.iter().map(|r| json!({
            "row": r,
            "record": record_of(app, sheet, *r, &table.headers, table.c0),
        })).collect::<Vec<_>>(),
    }))
}

fn table_update(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let table = table_of(app, sheet)?;
    let criteria = args
        .get("criteria")
        .and_then(Value::as_object)
        .context("missing \"criteria\"")?;
    let changes = args
        .get("changes")
        .and_then(Value::as_object)
        .context("missing \"changes\"")?;
    let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);

    let rows = matching_rows(app, sheet, &table, criteria)?;
    if rows.is_empty() {
        bail!("nothing matched those criteria — use grido_table_find to check first");
    }
    if rows.len() > 1 && !all {
        bail!(
            "{} rows match (rows {}). Narrow the criteria, or pass all=true to \
             update them together.",
            rows.len(),
            rows.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", ")
        );
    }

    app.engine.begin_transaction();
    for row in &rows {
        for (name, value) in changes {
            let col = column_of(&table.headers, name, table.c0)?;
            let text = match value {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            app.engine.set_input(sheet, *row, col, &text)?;
        }
    }
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
        app.note_change(
            sheet,
            Range { r0: *first, r1: *last, c0: table.c0, c1: table.c1() },
        );
    }
    Ok(json!({
        "updated": rows.len(),
        "rows": rows.iter().map(|r| json!({
            "row": r,
            "record": record_of(app, sheet, *r, &table.headers, table.c0),
        })).collect::<Vec<_>>(),
    }))
}

fn rows_insert(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let at = args.get("at").and_then(Value::as_i64).context("missing \"at\"")? as i32;
    let count = args.get("count").and_then(Value::as_i64).unwrap_or(1) as i32;
    app.engine.insert_rows(sheet, at, count)?;
    app.invalidate_geometry();
    Ok(json!(format!("inserted {count} row(s) at {at}")))
}

fn rows_delete(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let at = args.get("at").and_then(Value::as_i64).context("missing \"at\"")? as i32;
    let count = args.get("count").and_then(Value::as_i64).unwrap_or(1) as i32;
    // Destructive: hand it to the user rather than doing it behind their back.
    app.request_confirmation(
        format!("Delete {count} row(s) starting at row {at}?"),
        crate::state::AssistantAction::DeleteRows { sheet, at, count },
    );
    Ok(json!(
        "asked the user to confirm — the rows are deleted only if they approve"
    ))
}

fn sheet_add(app: &mut GridoApp, args: &Value) -> Result<Value> {
    app.engine.new_sheet()?;
    let index = app.engine.sheet_names().len() as u32 - 1;
    if let Some(name) = args.get("name").and_then(Value::as_str) {
        app.engine.rename_sheet(index, name)?;
    }
    Ok(json!(format!(
        "added sheet \"{}\"",
        app.engine.sheet_names()[index as usize]
    )))
}

fn sheet_rename(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let name = str_arg(args, "name")?;
    app.engine.rename_sheet(sheet, name)?;
    Ok(json!(format!("renamed to \"{name}\"")))
}

fn sort(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let mut range = range_of(args, "range")?;
    let by = str_arg(args, "by")?;
    let descending = args
        .get("descending")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let headers = args.get("headers").and_then(Value::as_bool).unwrap_or(true);

    // `by` is a column letter, or a header name to look up.
    let col = match parse_a1_range(&format!("{by}1")) {
        Some(r) if by.chars().all(|c| c.is_ascii_alphabetic()) => r.c0,
        _ => {
            let table = table_of(app, sheet)?;
            column_of(&table.headers, by, table.c0)?
        }
    };
    if headers && range.rows() > 1 {
        range.r0 += 1;
    }
    app.engine.begin_transaction();
    app.engine.sort_range(sheet, range, col, !descending)?;
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    app.note_change(sheet, range);
    Ok(json!(format!(
        "sorted {} by column {}",
        crate::engine::a1_range(range),
        col_letters(col)
    )))
}

fn find(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let text = str_arg(args, "text")?;
    let match_case = args
        .get("match_case")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let hits = app.engine.find(sheet, text, match_case, false);
    Ok(json!({
        "matches": hits.iter().take(200).map(|h| json!({
            "cell": format!("{}{}", col_letters(h.col), h.row),
            "value": app.engine.display(h.sheet, h.row, h.col),
        })).collect::<Vec<_>>(),
        "total": hits.len(),
    }))
}

fn stats(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let sheet = sheet_of(app, args)?;
    let range = range_of(args, "range")?;
    let mut values = Vec::new();
    for r in range.r0..=range.r1 {
        for c in range.c0..=range.c1 {
            if app.engine.is_number(sheet, r, c) {
                if let Ok(v) = app.engine.display(sheet, r, c).replace(',', "").parse::<f64>() {
                    values.push(v);
                }
            }
        }
    }
    if values.is_empty() {
        return Ok(json!({ "count": 0, "note": "no numbers in that range" }));
    }
    let sum: f64 = values.iter().sum();
    Ok(json!({
        "count": values.len(),
        "sum": sum,
        "average": sum / values.len() as f64,
        "min": values.iter().cloned().fold(f64::INFINITY, f64::min),
        "max": values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    }))
}
