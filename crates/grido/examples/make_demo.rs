//! Generates a demo workbook exercising values, formulas, styles, number
//! formats and conditional formatting: `cargo run --example make_demo [path]`

use ironcalc::base::cf_types::CfRuleInput;
use ironcalc::base::UserModel;
use ironcalc::export::save_xlsx_to_writer;
use std::io::BufWriter;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo.xlsx".to_string());
    let mut um = UserModel::new_empty("demo", "en", "UTC", "en").unwrap();

    // --- Sheet 1: a small sales table -------------------------------------
    let headers = ["Item", "Region", "Qty", "Unit price", "Total", "Margin"];
    for (c, h) in headers.iter().enumerate() {
        um.set_user_input(0, 1, c as i32 + 1, h).unwrap();
    }
    let rows = [
        ("Widgets", "North", 120, 3.50, 0.42),
        ("Gadgets", "South", 45, 12.00, 0.31),
        ("Sprockets", "North", 400, 0.85, 0.18),
        ("Doohickeys", "East", 72, 22.40, 0.55),
        ("Grommets", "West", 210, 1.75, 0.24),
        ("Flanges", "South", 18, 44.00, 0.61),
    ];
    for (i, (item, region, qty, price, margin)) in rows.iter().enumerate() {
        let r = i as i32 + 2;
        um.set_user_input(0, r, 1, item).unwrap();
        um.set_user_input(0, r, 2, region).unwrap();
        um.set_user_input(0, r, 3, &qty.to_string()).unwrap();
        um.set_user_input(0, r, 4, &price.to_string()).unwrap();
        um.set_user_input(0, r, 5, &format!("=C{r}*D{r}")).unwrap();
        um.set_user_input(0, r, 6, &margin.to_string()).unwrap();
    }
    let last = rows.len() as i32 + 1;
    um.set_user_input(0, last + 2, 1, "Grand total").unwrap();
    um.set_user_input(0, last + 2, 5, &format!("=SUM(E2:E{last})"))
        .unwrap();
    um.set_user_input(0, last + 3, 1, "Average price").unwrap();
    um.set_user_input(0, last + 3, 5, &format!("=AVERAGE(D2:D{last})"))
        .unwrap();
    um.set_user_input(0, last + 4, 1, "Best seller").unwrap();
    um.set_user_input(0, last + 4, 5, &format!("=MAX(E2:E{last})"))
        .unwrap();

    // Header row: bold, white on green, centered.
    let header = area(0, 1, 1, 6, 1);
    um.update_range_style(&header, "font.b", "true").unwrap();
    um.update_range_style(&header, "font.color", "#FFFFFF").unwrap();
    um.update_range_style(&header, "fill.color", "#1E7145").unwrap();
    um.update_range_style(&header, "alignment.horizontal", "center")
        .unwrap();

    // Number formats: currency for price/total, percent for margin.
    let money = area(0, 2, 4, 2, rows.len() as i32);
    um.update_range_style(&money, "num_fmt", "\"$\"#,##0.00")
        .unwrap();
    let pct = area(0, 2, 6, 1, rows.len() as i32);
    um.update_range_style(&pct, "num_fmt", "0.0%").unwrap();
    let totals = area(0, last + 2, 5, 1, 3);
    um.update_range_style(&totals, "num_fmt", "\"$\"#,##0.00")
        .unwrap();
    um.update_range_style(&area(0, last + 2, 1, 5, 3), "font.b", "true")
        .unwrap();

    // Conditional formatting: highlight large line totals.
    let rule = CfRuleInput::CellIs {
        operator: serde_json::from_str("\"GreaterThan\"").unwrap(),
        formula: "500".to_string(),
        formula2: None,
        format: serde_json::from_value(serde_json::json!({
            "fill": { "color": "#FFC7CE" },
            "font": { "color": "#9C0006" },
        }))
        .unwrap(),
        stop_if_true: false,
    };
    um.add_conditional_formatting(0, &format!("E2:E{last}"), rule)
        .unwrap();

    // Freeze the header row and widen the label column.
    um.set_frozen_rows_count(0, 1).unwrap();
    um.set_columns_width(0, 1, 1, 130.0).unwrap();
    um.set_columns_width(0, 4, 5, 110.0).unwrap();

    // --- Sheet 2: formula showcase ----------------------------------------
    um.new_sheet().unwrap();
    um.rename_sheet(1, "Formulas").unwrap();
    let samples = [
        ("Sum", "=SUM(1,2,3,4)"),
        ("Average", "=AVERAGE(10,20,30)"),
        ("Count", "=COUNT(1,2,3)"),
        ("Text join", "=CONCAT(\"Sheet\",\"z\")"),
        ("Upper", "=UPPER(\"native\")"),
        ("If", "=IF(5>3,\"yes\",\"no\")"),
        ("Lookup", "=VLOOKUP(2,{1,\"one\";2,\"two\"},2,FALSE)"),
        ("Round", "=ROUND(3.14159,2)"),
        ("Power", "=2^10"),
        ("Today", "=TODAY()"),
    ];
    um.set_user_input(1, 1, 1, "Function").unwrap();
    um.set_user_input(1, 1, 2, "Result").unwrap();
    um.set_user_input(1, 1, 3, "Formula").unwrap();
    let head2 = area(1, 1, 1, 3, 1);
    um.update_range_style(&head2, "font.b", "true").unwrap();
    um.update_range_style(&head2, "fill.color", "#DEEBF7").unwrap();
    for (i, (name, formula)) in samples.iter().enumerate() {
        let r = i as i32 + 2;
        um.set_user_input(1, r, 1, name).unwrap();
        um.set_user_input(1, r, 2, formula).unwrap();
        // Third column shows the formula text itself.
        um.set_user_input(1, r, 3, &format!("'{formula}")).unwrap();
    }
    um.set_columns_width(1, 3, 3, 220.0).unwrap();

    um.evaluate();
    let file = std::fs::File::create(&path).unwrap();
    save_xlsx_to_writer(um.get_model(), BufWriter::new(file)).unwrap();
    println!("wrote {path}");
}

fn area(
    sheet: u32,
    row: i32,
    column: i32,
    width: i32,
    height: i32,
) -> ironcalc::base::expressions::types::Area {
    ironcalc::base::expressions::types::Area {
        sheet,
        row,
        column,
        width,
        height,
    }
}
