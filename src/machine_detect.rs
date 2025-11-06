use once_cell::sync::OnceCell;
use std::fs;
use std::path::Path;

static COMPUTER_MODEL: OnceCell<Option<ComputerModel>> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerModel {
    DatorBBFält,
    DatorBBFältGPS,
    EjKänd,
}

pub fn has_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBBFält => true,
        ComputerModel::DatorBBFältGPS => true,
        ComputerModel::EjKänd => false,
    }
}

pub fn has_serial_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBBFält => true,
        _ => false,
    }
}

fn read_trim<P: AsRef<Path>>(p: P) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

pub fn get_computer_model() -> ComputerModel {
    if let Some(cached) = COMPUTER_MODEL.get().and_then(|opt| *opt) {
        return cached;
    }

    let read_model = read_computer_model();

    COMPUTER_MODEL.set(Some(read_model)).unwrap();

    return read_model;
}

fn read_computer_model() -> ComputerModel {
    let mut model = ComputerModel::EjKänd;

    if let Some(board_name) = read_trim("/sys/class/dmi/id/board_name") {
        match board_name.as_str() {
            "DR786EX" | "CAPELL VALLEY(NAPA) CRB" => model = ComputerModel::DatorBBFält,
            _ => {}
        };
    }

    if let Some(product_name) = read_trim("/sys/class/dmi/id/product_name") {
        match product_name.as_str() {
            "DT10" => model = ComputerModel::DatorBBFältGPS,
            _ => {}
        }
    }

    return model;
}
