//! Ready-made workbooks.
//!
//! "Make me a job application tracker" should produce something usable in one
//! turn — headers, sensible formats, a frozen header row and a few summary
//! formulas — rather than a bare grid the assistant then has to decorate.

use anyhow::{bail, Context as _, Result};
use serde_json::{json, Value};

use crate::engine::{HAlign, Range, StyleEdit};
use crate::state::GridoApp;

struct Template {
    sheet: &'static str,
    headers: &'static [&'static str],
    /// (column index 1-based, number format)
    formats: &'static [(i32, &'static str)],
    /// (label, formula) rows written a couple of rows under the table
    summary: &'static [(&'static str, &'static str)],
}

const JOB_APPLICATIONS: Template = Template {
    sheet: "Applications",
    headers: &[
        "Company", "Role", "Applied", "Status", "Source", "Contact", "Next step", "Notes",
    ],
    formats: &[(3, "yyyy-mm-dd")],
    summary: &[
        ("Total applications", "=COUNTA(A2:A1000)"),
        ("Interviews", "=COUNTIF(D2:D1000,\"Interview\")"),
        ("Offers", "=COUNTIF(D2:D1000,\"Offer\")"),
        ("Rejected", "=COUNTIF(D2:D1000,\"Rejected\")"),
    ],
};

const BUDGET: Template = Template {
    sheet: "Budget",
    headers: &["Date", "Category", "Description", "Amount"],
    formats: &[(1, "yyyy-mm-dd"), (4, "\"$\"#,##0.00")],
    summary: &[
        ("Total", "=SUM(D2:D1000)"),
        ("Entries", "=COUNT(D2:D1000)"),
        ("Largest", "=MAX(D2:D1000)"),
    ],
};

const CONTACTS: Template = Template {
    sheet: "Contacts",
    headers: &["Name", "Email", "Phone", "Company", "Notes"],
    formats: &[],
    summary: &[("Contacts", "=COUNTA(A2:A1000)")],
};

pub fn create(app: &mut GridoApp, args: &Value) -> Result<Value> {
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .context("missing \"kind\"")?;
    let template = match kind {
        "job-applications" => JOB_APPLICATIONS,
        "budget" => BUDGET,
        "contacts" => CONTACTS,
        other => bail!("unknown template \"{other}\" (job-applications, budget, contacts)"),
    };

    if app.engine.dirty {
        bail!("the open workbook has unsaved changes — save it before starting a new one");
    }
    app.new_workbook();
    let sheet = 0;
    app.engine.rename_sheet(sheet, template.sheet)?;
    app.engine.begin_transaction();

    for (i, name) in template.headers.iter().enumerate() {
        app.engine.set_input(sheet, 1, i as i32 + 1, name)?;
    }
    let width = template.headers.len() as i32;
    let header = Range { r0: 1, r1: 1, c0: 1, c1: width };
    app.engine.set_styles(
        sheet,
        header,
        &[
            StyleEdit::Bold(true),
            StyleEdit::FontColor(Some("#FFFFFF".into())),
            StyleEdit::FillColor(Some("#1E7145".into())),
            StyleEdit::Align(HAlign::Center),
        ],
    )?;
    for (col, code) in template.formats {
        let range = Range { r0: 2, r1: 400, c0: *col, c1: *col };
        app.engine
            .set_style(sheet, range, StyleEdit::NumFmt((*code).to_string()))?;
    }

    // Summary block, a couple of rows clear of the table.
    let start = 3;
    for (i, (label, formula)) in template.summary.iter().enumerate() {
        let row = 500 + i as i32;
        let _ = row; // summaries live near the top-right instead
        let r = start + i as i32;
        app.engine.set_input(sheet, r, width + 2, label)?;
        app.engine.set_input(sheet, r, width + 3, formula)?;
    }
    let labels = Range {
        r0: start,
        r1: start + template.summary.len() as i32 - 1,
        c0: width + 2,
        c1: width + 2,
    };
    app.engine.set_style(sheet, labels, StyleEdit::Bold(true))?;

    app.engine.set_frozen(sheet, 1, 0)?;
    app.engine.set_col_width(sheet, 1, width, 120.0)?;
    let steps = app.engine.end_transaction();
    app.push_undo_group(steps);
    app.reset_view();

    // Save straight away when given a path: a template the user cannot find
    // again is not much use.
    let saved = match args.get("path").and_then(Value::as_str) {
        Some(p) => {
            let mut path = std::path::PathBuf::from(p);
            if path.extension().is_none() {
                path.set_extension("xlsx");
            }
            app.engine.save(&path)?;
            Some(path.display().to_string())
        }
        None => None,
    };

    Ok(json!({
        "created": kind,
        "sheet": template.sheet,
        "columns": template.headers,
        "saved_to": saved,
    }))
}
