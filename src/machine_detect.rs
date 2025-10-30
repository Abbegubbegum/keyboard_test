use once_cell::sync::OnceCell;
use std::fs;
use std::path::Path;

static COMPUTER_MODEL: OnceCell<Option<ComputerModel>> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerModel {
    DatorBBFält786,
    DatorBBFält886,
    EjKänd,
}

pub fn has_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBBFält786 => true,
        ComputerModel::DatorBBFält886 => true,
        ComputerModel::EjKänd => false,
    }
}

pub fn has_serial_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBBFält786 => true,
        ComputerModel::DatorBBFält886 => true,
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

    if let Some(model) = read_trim("/sys/class/dmi/id/board_name") {
        match model.as_str() {
            "DR786EX" => {
                COMPUTER_MODEL
                    .set(Some(ComputerModel::DatorBBFält786))
                    .unwrap();
                ComputerModel::DatorBBFält786
            }
            "CAPELL VALLEY(NAPA) CRB" => {
                COMPUTER_MODEL
                    .set(Some(ComputerModel::DatorBBFält886))
                    .unwrap();
                ComputerModel::DatorBBFält886
            }
            _ => {
                COMPUTER_MODEL.set(Some(ComputerModel::EjKänd)).unwrap();
                ComputerModel::EjKänd
            }
        }
    } else {
        COMPUTER_MODEL.set(Some(ComputerModel::EjKänd)).unwrap();
        ComputerModel::EjKänd
    }
}
