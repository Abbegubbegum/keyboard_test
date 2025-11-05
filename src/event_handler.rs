use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::{fs, vec};
use std::{thread, time::Duration};

use crate::serial_touch;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug)]
pub enum AppEvent {
    Key {
        code: KeyCode,
        info: DeviceInfo,
    },
    Mouse {
        x: i16,
        y: i16,
        info: DeviceInfo,
    },
    Touch {
        x: u16,
        y: u16,
        timestamp: u128,
        released: bool,
    },
}

pub fn spawn_device_listeners(tx: &Sender<AppEvent>) -> Result<()> {
    let devices = get_devices();

    if devices.is_empty() {
        return Err(eyre!(
            "no input devices found, ensure you have the necessary permissions"
        ));
    }

    // Track active device paths to avoid duplicate listeners
    let active_devices = Arc::new(Mutex::new(HashSet::new()));

    // Spawn initial device listeners
    for (dev, info) in devices {
        let path = info.path.clone();
        if let Ok(mut set) = active_devices.lock() {
            set.insert(path.clone());
        }
        spawn_device_listener(dev, info, tx.clone(), active_devices.clone());
    }

    // Spawn hotswap monitor thread
    let tx_clone = tx.clone();
    let active_devices_clone = active_devices.clone();
    thread::spawn(move || {
        hotswap_monitor(tx_clone, active_devices_clone);
    });

    let tx_clone = tx.clone();
    let _ = serial_touch::spawn_reader(tx_clone);

    Ok(())
}

fn spawn_device_listener(
    mut dev: Device,
    info: DeviceInfo,
    tx: Sender<AppEvent>,
    active_devices: Arc<Mutex<HashSet<String>>>,
) {
    let path = info.path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100)); // Allow some stagger time

        loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for event in events {
                        match event.destructure() {
                            EventSummary::Key(_, code, 1) => {
                                _ = tx.send(AppEvent::Key {
                                    code,
                                    info: info.clone(),
                                });
                            }
                            // Handle mouse movement events
                            EventSummary::RelativeAxis(_, rel_code, value) => {
                                if rel_code == evdev::RelativeAxisCode::REL_X {
                                    // X movement
                                    _ = tx.send(AppEvent::Mouse {
                                        x: value as i16,
                                        y: 0,
                                        info: info.clone(),
                                    });
                                } else if rel_code == evdev::RelativeAxisCode::REL_Y {
                                    // Y movement
                                    _ = tx.send(AppEvent::Mouse {
                                        x: 0,
                                        y: value as i16,
                                        info: info.clone(),
                                    });
                                }
                            }
                            _ => {
                                continue;
                            }
                        }
                    }
                }
                Err(e) => {
                    // Device disconnected or error occurred
                    // Error 19 (ENODEV - No such device) means device was unplugged
                    let is_disconnect = e.kind() == std::io::ErrorKind::NotFound
                        || e.kind() == std::io::ErrorKind::Other
                        || e.raw_os_error() == Some(19); // ENODEV

                    if !is_disconnect {
                        eprintln!("Error fetching events from device {}: {}", info.name, e);
                    }
                    // Remove from active devices set
                    if let Ok(mut set) = active_devices.lock() {
                        set.remove(&path);
                    }
                    break; // Exit the loop on error
                }
            }
        }
    });
}

fn hotswap_monitor(tx: Sender<AppEvent>, active_devices: Arc<Mutex<HashSet<String>>>) {
    loop {
        thread::sleep(Duration::from_secs(2)); // Check every 2 seconds

        let devices = get_devices();

        for (dev, info) in devices {
            let path = info.path.clone();

            // Check if this device is already being monitored
            let is_new = if let Ok(set) = active_devices.lock() {
                !set.contains(&path)
            } else {
                false
            };

            if is_new {
                // New device detected, spawn listener for it
                if let Ok(mut set) = active_devices.lock() {
                    set.insert(path.clone());
                }
                spawn_device_listener(dev, info, tx.clone(), active_devices.clone());
            }
        }
    }
}

fn get_devices() -> Vec<(Device, DeviceInfo)> {
    let mut devices: Vec<(Device, DeviceInfo)> = vec![];

    let dir = fs::read_dir("/dev/input").expect("Failed to read /dev/input directory");

    for entry in dir.filter_map(Result::ok) {
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }

        match Device::open(entry.path()) {
            Ok(device) => {
                let name = device.name().unwrap_or("Unknown").to_string();

                devices.push((
                    device,
                    DeviceInfo {
                        path: entry.path().to_string_lossy().to_string(),
                        name,
                    },
                ))
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
