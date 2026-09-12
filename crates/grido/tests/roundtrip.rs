use ironcalc::base::UserModel;
use ironcalc::export::save_xlsx_to_writer;
use ironcalc::import::load_from_xlsx;
use std::io::BufWriter;

/// Values and formulas survive a save → load round trip.
#[test]
fn xlsx_roundtrip() {
    let mut um = UserModel::new_empty("Book1", "en", "UTC", "en").unwrap();
    um.set_user_input(0, 1, 1, "41").unwrap();
    um.set_user_input(0, 2, 1, "1").unwrap();
    um.set_user_input(0, 3, 1, "=SUM(A1:A2)").unwrap();
    um.set_user_input(0, 1, 2, "hello").unwrap();
    assert_eq!(um.get_formatted_cell_value(0, 3, 1).unwrap(), "42");

    let dir = std::env::temp_dir().join("grido-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.xlsx");
    let file = std::fs::File::create(&path).unwrap();
    save_xlsx_to_writer(um.get_model(), BufWriter::new(file)).unwrap();

    let model = load_from_xlsx(path.to_str().unwrap(), "en", "UTC", "en").unwrap();
    let um2 = UserModel::from_model(model);
    assert_eq!(um2.get_formatted_cell_value(0, 3, 1).unwrap(), "42");
    assert_eq!(um2.get_cell_content(0, 3, 1).unwrap(), "=SUM(A1:A2)");
    assert_eq!(um2.get_formatted_cell_value(0, 1, 2).unwrap(), "hello");
    std::fs::remove_file(&path).ok();
}
