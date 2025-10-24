use once_cell::sync::OnceCell;
use std::fs;
use std::path::Path;

static COMPUTER_MODEL: OnceCell<Option<ComputerModel>> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerModel {
    DatorBärbarFält,
    EjKänd,
}

pub fn has_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBärbarFält => true,
        ComputerModel::EjKänd => false,
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
                    .set(Some(ComputerModel::DatorBärbarFält))
                    .unwrap();
                ComputerModel::DatorBärbarFält
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
