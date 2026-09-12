//! Engine-level integration tests: the behaviors the UI depends on.

use grido::engine::{BorderTarget, Engine, HAlign, Range, StyleEdit};

fn engine() -> Engine {
    Engine::new().expect("empty workbook")
}

fn put(e: &mut Engine, row: i32, col: i32, value: &str) {
    e.set_input(0, row, col, value).expect("set input");
}

#[test]
fn formulas_evaluate_and_update() {
    let mut e = engine();
    put(&mut e, 1, 1, "20");
    put(&mut e, 2, 1, "22");
    put(&mut e, 3, 1, "=SUM(A1:A2)");
    assert_eq!(e.display(0, 3, 1), "42");

    // Changing an input recalculates dependents.
    put(&mut e, 1, 1, "30");
    assert_eq!(e.display(0, 3, 1), "52");
    assert_eq!(e.content(0, 3, 1), "=SUM(A1:A2)");
}

#[test]
fn styles_round_trip_through_the_format_api() {
    let mut e = engine();
    put(&mut e, 1, 1, "hello");
    let cell = Range::single(1, 1);
    e.set_style(0, cell, StyleEdit::Bold(true)).unwrap();
    e.set_style(0, cell, StyleEdit::Italic(true)).unwrap();
    e.set_style(0, cell, StyleEdit::FontSize(18)).unwrap();
    e.set_style(0, cell, StyleEdit::FillColor(Some("#FF0000".into())))
        .unwrap();
    e.set_style(0, cell, StyleEdit::Align(HAlign::Center)).unwrap();

    let fmt = e.format(0, 1, 1);
    assert!(fmt.bold, "bold should stick");
    assert!(fmt.italic);
    assert_eq!(fmt.font_size, 18.0);
    assert_eq!(fmt.fill_color, Some([255, 0, 0]));
    assert_eq!(fmt.align, HAlign::Center);
}

#[test]
fn borders_apply_to_a_range() {
    let mut e = engine();
    let range = Range {
        r0: 1,
        r1: 2,
        c0: 1,
        c1: 2,
    };
    e.set_border(0, range, BorderTarget::All, "#000000").unwrap();
    let fmt = e.format(0, 1, 1);
    assert!(
        fmt.borders.iter().any(|b| b.is_some()),
        "expected at least one border edge on a bordered cell"
    );
}

#[test]
fn insert_and_delete_rows_shift_data_and_fix_formulas() {
    let mut e = engine();
    put(&mut e, 1, 1, "first");
    put(&mut e, 2, 1, "second");
    put(&mut e, 3, 1, "=A1");

    e.insert_rows(0, 1, 1).unwrap();
    assert_eq!(e.display(0, 1, 1), "", "a blank row was inserted at the top");
    assert_eq!(e.display(0, 2, 1), "first");
    // The formula moved down and still points at "first".
    assert_eq!(e.display(0, 4, 1), "first");

    e.delete_rows(0, 1, 1).unwrap();
    assert_eq!(e.display(0, 1, 1), "first");
}

#[test]
fn sorting_reorders_rows() {
    let mut e = engine();
    for (i, name) in ["cherry", "apple", "banana"].iter().enumerate() {
        put(&mut e, i as i32 + 1, 1, name);
    }
    let range = Range {
        r0: 1,
        r1: 3,
        c0: 1,
        c1: 1,
    };
    e.sort_range(0, range, 1, true).unwrap();
    assert_eq!(e.display(0, 1, 1), "apple");
    assert_eq!(e.display(0, 2, 1), "banana");
    assert_eq!(e.display(0, 3, 1), "cherry");

    e.sort_range(0, range, 1, false).unwrap();
    assert_eq!(e.display(0, 1, 1), "cherry");
}

#[test]
fn sorting_compares_numbers_numerically() {
    let mut e = engine();
    for (i, n) in ["100", "9", "50"].iter().enumerate() {
        put(&mut e, i as i32 + 1, 1, n);
    }
    let range = Range {
        r0: 1,
        r1: 3,
        c0: 1,
        c1: 1,
    };
    e.sort_range(0, range, 1, true).unwrap();
    // Lexicographic order would give 100, 50, 9 — we want numeric order.
    assert_eq!(e.display(0, 1, 1), "9");
    assert_eq!(e.display(0, 2, 1), "50");
    assert_eq!(e.display(0, 3, 1), "100");
}

#[test]
fn find_and_replace_across_the_sheet() {
    let mut e = engine();
    put(&mut e, 1, 1, "Widget");
    put(&mut e, 2, 1, "widget");
    put(&mut e, 3, 1, "Gadget");

    assert_eq!(e.find(0, "widget", false, false).len(), 2);
    assert_eq!(e.find(0, "widget", true, false).len(), 1);

    let n = e.replace_all(0, "widget", "sprocket", false, false);
    assert_eq!(n, 2);
    assert_eq!(e.display(0, 1, 1), "sprocket");
    assert_eq!(e.display(0, 3, 1), "Gadget");
}

#[test]
fn undo_and_redo_restore_values() {
    let mut e = engine();
    put(&mut e, 1, 1, "one");
    put(&mut e, 1, 1, "two");
    assert_eq!(e.display(0, 1, 1), "two");

    e.undo();
    assert_eq!(e.display(0, 1, 1), "one");
    e.redo();
    assert_eq!(e.display(0, 1, 1), "two");
}

#[test]
fn fill_down_adjusts_relative_references() {
    let mut e = engine();
    put(&mut e, 1, 1, "2");
    put(&mut e, 2, 1, "3");
    put(&mut e, 1, 2, "=A1*10");

    let src = Range {
        r0: 1,
        r1: 1,
        c0: 2,
        c1: 2,
    };
    e.auto_fill_rows(0, src, 2).unwrap();
    assert_eq!(e.display(0, 2, 2), "30", "filled formula should reference A2");
}

#[test]
fn clipboard_copy_paste_keeps_formulas_relative() {
    let mut e = engine();
    put(&mut e, 1, 1, "5");
    put(&mut e, 2, 1, "7");
    put(&mut e, 1, 2, "=A1*2");

    let src = Range::single(1, 2);
    let tsv = e.copy(0, src).unwrap();
    assert_eq!(tsv.trim(), "10");

    e.paste_internal(0, (2, 2), false).unwrap();
    // Pasted one row down, so the reference should follow to A2.
    assert_eq!(e.display(0, 2, 2), "14");
}

#[test]
fn sheets_can_be_added_renamed_and_removed() {
    let mut e = engine();
    e.new_sheet().unwrap();
    assert_eq!(e.sheet_names().len(), 2);

    e.rename_sheet(1, "Budget").unwrap();
    assert_eq!(e.sheet_names()[1], "Budget");

    e.duplicate_sheet(1).unwrap();
    assert_eq!(e.sheet_names().len(), 3);

    e.delete_sheet(1).unwrap();
    assert_eq!(e.sheet_names().len(), 2);
    assert!(!e.sheet_names().contains(&"Budget".to_string()));
}

#[test]
fn defined_names_can_be_added_and_deleted() {
    let mut e = engine();
    put(&mut e, 1, 1, "10");
    put(&mut e, 2, 1, "32");
    let sheet = e.sheet_names()[0].clone();

    // Defined names must refer to a range, not a bare constant.
    e.add_defined_name("Inputs", &format!("={sheet}!$A$1:$A$2"))
        .unwrap();
    assert!(e.defined_names().iter().any(|(n, _)| n == "Inputs"));

    // And the name is usable from a formula.
    put(&mut e, 4, 1, "=SUM(Inputs)");
    assert_eq!(e.display(0, 4, 1), "42");

    e.delete_defined_name("Inputs").unwrap();
    assert!(e.defined_names().iter().all(|(n, _)| n != "Inputs"));
}

#[test]
fn hiding_rows_reports_zero_height() {
    let mut e = engine();
    put(&mut e, 1, 1, "visible");
    put(&mut e, 2, 1, "hidden");
    e.set_rows_hidden(0, 2, 2, true).unwrap();
    assert!(e.is_row_hidden(0, 2));
    assert_eq!(e.row_height(0, 2), 0.0);
    assert!(e.row_height(0, 1) > 0.0);

    e.set_rows_hidden(0, 2, 2, false).unwrap();
    assert!(!e.is_row_hidden(0, 2));
}

#[test]
fn freeze_panes_persist_on_the_sheet() {
    let mut e = engine();
    e.set_frozen(0, 2, 1).unwrap();
    assert_eq!(e.frozen(0), (2, 1));
    e.set_frozen(0, 0, 0).unwrap();
    assert_eq!(e.frozen(0), (0, 0));
}

#[test]
fn conditional_formatting_colors_matching_cells() {
    let mut e = engine();
    put(&mut e, 1, 1, "5");
    put(&mut e, 2, 1, "500");
    let range = Range {
        r0: 1,
        r1: 2,
        c0: 1,
        c1: 1,
    };
    e.add_cf_cell_is(0, range, "greater", "100", None, "#FFC7CE")
        .unwrap();

    assert_eq!(e.cf_rules(0).len(), 1);
    // The extended style should paint the matching cell only.
    assert_eq!(e.format(0, 2, 1).fill_color, Some([255, 199, 206]));
    assert_eq!(e.format(0, 1, 1).fill_color, None);
}

#[test]
fn selection_statistics_sum_only_numbers() {
    let mut e = engine();
    put(&mut e, 1, 1, "10");
    put(&mut e, 2, 1, "20");
    put(&mut e, 3, 1, "label");
    let range = Range {
        r0: 1,
        r1: 3,
        c0: 1,
        c1: 1,
    };
    let (count, numeric, sum, avg) = e.stats(0, range);
    assert_eq!(count, 3);
    assert_eq!(numeric, 2);
    assert_eq!(sum, 30.0);
    assert_eq!(avg, 15.0);
}

#[test]
fn number_formats_change_the_displayed_value() {
    let mut e = engine();
    put(&mut e, 1, 1, "0.5");
    let cell = Range::single(1, 1);
    e.set_style(0, cell, StyleEdit::NumFmt("0.00%".into())).unwrap();
    assert_eq!(e.display(0, 1, 1), "50.00%");
}

#[test]
fn saving_and_reloading_preserves_values_formulas_and_styles() {
    let mut e = engine();
    put(&mut e, 1, 1, "41");
    put(&mut e, 2, 1, "1");
    put(&mut e, 3, 1, "=SUM(A1:A2)");
    e.set_style(0, Range::single(1, 1), StyleEdit::Bold(true))
        .unwrap();
    e.set_style(
        0,
        Range::single(1, 1),
        StyleEdit::FillColor(Some("#00FF00".into())),
    )
    .unwrap();
    e.rename_sheet(0, "Data").unwrap();

    let dir = std::env::temp_dir().join("grido-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("features.xlsx");
    e.save(&path).unwrap();
    assert!(!e.dirty, "saving clears the dirty flag");

    let reloaded = Engine::open(&path).unwrap();
    assert_eq!(reloaded.display(0, 3, 1), "42");
    assert_eq!(reloaded.content(0, 3, 1), "=SUM(A1:A2)");
    assert_eq!(reloaded.sheet_names()[0], "Data");
    let fmt = reloaded.format(0, 1, 1);
    assert!(fmt.bold, "bold should survive a save/load round trip");
    assert_eq!(fmt.fill_color, Some([0, 255, 0]));

    std::fs::remove_file(&path).ok();
}

#[test]
fn saving_twice_overwrites_rather_than_failing() {
    let mut e = engine();
    put(&mut e, 1, 1, "first");
    let dir = std::env::temp_dir().join("grido-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("overwrite.xlsx");
    e.save(&path).unwrap();
    put(&mut e, 1, 1, "second");
    e.save(&path).expect("second save must overwrite");

    let reloaded = Engine::open(&path).unwrap();
    assert_eq!(reloaded.display(0, 1, 1), "second");
    std::fs::remove_file(&path).ok();
}

#[test]
fn multi_key_sort_orders_by_each_key_in_turn() {
    let mut e = engine();
    // (region, amount) — sort by region A→Z, then amount high→low.
    let data = [
        ("South", "10"),
        ("North", "5"),
        ("South", "30"),
        ("North", "20"),
    ];
    for (i, (region, amount)) in data.iter().enumerate() {
        let r = i as i32 + 1;
        put(&mut e, r, 1, region);
        put(&mut e, r, 2, amount);
    }
    let range = Range {
        r0: 1,
        r1: 4,
        c0: 1,
        c1: 2,
    };
    e.sort_range_by(0, range, &[(1, true), (2, false)]).unwrap();

    let rows: Vec<(String, String)> = (1..=4)
        .map(|r| (e.display(0, r, 1), e.display(0, r, 2)))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("North".to_string(), "20".to_string()),
            ("North".to_string(), "5".to_string()),
            ("South".to_string(), "30".to_string()),
            ("South".to_string(), "10".to_string()),
        ]
    );
}

#[test]
fn sorting_pushes_blanks_to_the_end() {
    let mut e = engine();
    put(&mut e, 1, 1, "b");
    put(&mut e, 3, 1, "a");
    let range = Range {
        r0: 1,
        r1: 3,
        c0: 1,
        c1: 1,
    };
    e.sort_range(0, range, 1, true).unwrap();
    assert_eq!(e.display(0, 1, 1), "a");
    assert_eq!(e.display(0, 2, 1), "b");
    assert_eq!(e.display(0, 3, 1), "");
}

#[test]
fn repeated_identical_style_writes_are_ignored() {
    let mut e = engine();
    put(&mut e, 1, 1, "x");
    let cell = Range::single(1, 1);
    e.set_style(0, cell, StyleEdit::Bold(true)).unwrap();
    // Writing the same value again must not push another undo entry.
    e.set_style(0, cell, StyleEdit::Bold(true)).unwrap();
    e.undo();
    assert!(!e.format(0, 1, 1).bold, "one undo should clear the bold");
}

#[test]
fn defined_names_accept_a_leading_equals() {
    let mut e = engine();
    put(&mut e, 1, 1, "1");
    let sheet = e.sheet_names()[0].clone();
    e.add_defined_name("WithEquals", &format!("={sheet}!$A$1"))
        .unwrap();
    e.add_defined_name("WithoutEquals", &format!("{sheet}!$A$1"))
        .unwrap();
    assert_eq!(e.defined_names().len(), 2);
}
