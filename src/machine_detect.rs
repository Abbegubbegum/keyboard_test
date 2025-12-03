use once_cell::sync::OnceCell;
use std::fs;
use std::path::Path;

static COMPUTER_MODEL: OnceCell<Option<ComputerModel>> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerModel {
    DatorBBFält,
    DatorBBFältGPS,
    DatorBärbarRS11,
    DatorBärbarCMBRF8,
    EjKänd,
}

pub fn has_touchscreen(c: ComputerModel) -> bool {
    match c {
        ComputerModel::DatorBBFält => true,
        ComputerModel::DatorBBFältGPS => true,
        _ => false,
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

fn has_cypress_device() -> bool {
    if let Ok(input_devices) = fs::read_to_string("/proc/bus/input/devices") {
        return input_devices.contains("Cypress");
    }
    false
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
            "DR786EX" => model = ComputerModel::DatorBBFält,
            "CAPELL VALLEY(NAPA) CRB" => {
                // Both DatorBBFält and DatorBärbarCMBRF8 have identical DMI info
                // Differentiate by checking for Cypress device (present on DatorBBFält)
                if has_cypress_device() {
                    model = ComputerModel::DatorBBFält;
                } else {
                    model = ComputerModel::DatorBärbarCMBRF8;
                }
            }
            _ => {}
        };
    }

    if let Some(product_name) = read_trim("/sys/class/dmi/id/product_name") {
        match product_name.as_str() {
            "DT10" => model = ComputerModel::DatorBBFältGPS,
            "RS11" => model = ComputerModel::DatorBärbarRS11,
            _ => {}
        }
    }

    return model;
}
