//! Prints a cell: `cargo run --example dump_cell file.xlsx <row> <col>`
use ironcalc::base::UserModel;
use ironcalc::import::load_from_xlsx;

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("file");
    let row: i32 = args.next().expect("row").parse().unwrap();
    let col: i32 = args.next().expect("col").parse().unwrap();
    let model = load_from_xlsx(&file, "en", "UTC", "en").unwrap();
    let um = UserModel::from_model(model);
    println!(
        "content={:?} value={:?}",
        um.get_cell_content(0, row, col).unwrap(),
        um.get_formatted_cell_value(0, row, col).unwrap()
    );
}
