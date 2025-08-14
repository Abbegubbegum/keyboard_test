use crossbeam_channel::{Receiver, unbounded};
use evdev::{Device, EventSummary, KeyCode};
use nix::poll::PollFd;
use std::{fs, vec};

#[derive(Debug)]
pub struct AppDevice {
    pub path: String,
    pub name: String,
}

#[derive(Debug)]
pub enum AppEvent {
    Key { code: KeyCode, device: AppDevice },
    Mouse { x: i32, y: i32, device: AppDevice },
}

pub fn spawn_evdev_thread() -> Receiver<AppEvent> {
    let (tx, rx) = unbounded();

    std::thread::spawn(move || {
        let devices = get_devices();

        let mut fds: Vec<PollFd> = devices.iter().map();
    });

    return rx;
}

fn get_devices() -> Vec<(AppDevice, Device)> {
    let mut devices: Vec<(AppDevice, Device)> = vec![];

    let dir = fs::read_dir("/dev/input").expect("Failed to read /dev/input directory");

    for entry in dir.filter_map(Result::ok) {
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }

        match Device::open(entry.path()) {
            Ok(device) => {
                let name = device.name().unwrap_or("Unknown").to_string();

                devices.push((
                    AppDevice {
                        path: entry.path().to_string_lossy().to_string(),
                        name,
                    },
                    device,
                ))

                /*
                // A way to check if the device is a keyboard is to check if supported keys include KEY_A
                if device
                    .supported_keys()
                    .map_or(false, |keys| keys.contains(KeyCode::KEY_A))
                {
                    devices.push(KeyboardDevice { path, name });
                }
                 */
            }
            Err(error) => {
                // Ignore devices that cannot be opened
                eprintln!(
                    "Could not open device {}: {}",
                    entry.path().to_string_lossy(),
                    error
                );
                continue;
            } // Skip devices that cannot be opened
        }
    }

    return devices;
}
