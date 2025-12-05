use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::{fs, vec};
use std::{thread, time::Duration};

use crate::machine_detect::{ComputerModel, get_computer_model};
use crate::serial_touch;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
    pub abs_x_max: Option<i32>,
    pub abs_y_max: Option<i32>,
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
        info: Option<DeviceInfo>,
    },
    Tick,
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

    // Spawn timer thread for regular UI updates (needed for hold progress during calibration)
    let tx_timer = tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(100)); // 10 times per second
            let _ = tx_timer.send(AppEvent::Tick);
        }
    });

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

        // USB touchscreen/stylus state tracking
        let mut touch_x: u16 = 0;
        let mut touch_y: u16 = 0;
        let mut is_touching: bool = false; // Track whether stylus/finger is actually touching
        let mut tool_in_range: bool = false; // Track whether tool (pen/finger) is in range
        let mut coords_updated: bool = false; // Track if coordinates were updated in this event batch

        loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for event in events {
                        match event.destructure() {
                            EventSummary::Key(_, code, value) => {
                                // Handle various touch/stylus button codes
                                match code {
                                    // BTN_TOUCH: Actual contact with surface (both finger and stylus)
                                    KeyCode::BTN_TOUCH => {
                                        is_touching = value != 0;
                                        if !is_touching && tool_in_range {
                                            // Released but tool still in range - send release event
                                            _ = tx.send(get_touch_event(
                                                touch_x,
                                                touch_y,
                                                true,
                                                Some(info.clone()),
                                            ));
                                        }
                                    }
                                    // BTN_TOOL_PEN, BTN_TOOL_FINGER: Tool in range but not necessarily touching
                                    KeyCode::BTN_TOOL_PEN | KeyCode::BTN_TOOL_FINGER => {
                                        tool_in_range = value != 0;
                                        if !tool_in_range && is_touching {
                                            // Tool left range - send release event
                                            is_touching = false;
                                            _ = tx.send(get_touch_event(
                                                touch_x,
                                                touch_y,
                                                true,
                                                Some(info.clone()),
                                            ));
                                        }
                                    }
                                    // Regular key presses (only on press, not release)
                                    _ => {
                                        if value == 1 {
                                            _ = tx.send(AppEvent::Key {
                                                code,
                                                info: info.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                            // Handle USB touchscreen/stylus absolute axis events
                            EventSummary::AbsoluteAxis(_, abs_code, value) => match abs_code {
                                evdev::AbsoluteAxisCode::ABS_X => {
                                    touch_x = value as u16;
                                    coords_updated = true;
                                }
                                evdev::AbsoluteAxisCode::ABS_Y => {
                                    touch_y = value as u16;
                                    coords_updated = true;
                                }
                                // Ignore other axis events (pressure, tilt, etc.)
                                _ => {}
                            },
                            // EV_SYN marks the end of a complete event frame
                            EventSummary::Synchronization(_, sync_code, _) => {
                                if sync_code == evdev::SynchronizationCode::SYN_REPORT {
                                    // Send touch event only once per complete frame, if coordinates changed
                                    if is_touching && coords_updated {
                                        _ = tx.send(get_touch_event(
                                            touch_x,
                                            touch_y,
                                            false,
                                            Some(info.clone()),
                                        ));
                                        coords_updated = false;
                                    }
                                }
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

fn get_touch_event(x: u16, y: u16, released: bool, info: Option<DeviceInfo>) -> AppEvent {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    // For some reason, on the GPS touchpad, x is y and y is x
    if get_computer_model() == ComputerModel::DatorBBFältGPS {
        AppEvent::Touch {
            x: y,
            y: x,
            timestamp,
            released,
            info,
        }
    } else {
        AppEvent::Touch {
            x,
            y,
            timestamp,
            released,
            info,
        }
    }
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

                // Query absolute axis information for touchscreens/touchpads
                let abs_x_max = device.get_abs_state().ok().and_then(|abs_state| {
                    abs_state
                        .get(evdev::AbsoluteAxisCode::ABS_X.0 as usize)
                        .map(|info| info.maximum)
                });

                let abs_y_max = device.get_abs_state().ok().and_then(|abs_state| {
                    abs_state
                        .get(evdev::AbsoluteAxisCode::ABS_Y.0 as usize)
                        .map(|info| info.maximum)
                });

                devices.push((
                    device,
                    DeviceInfo {
                        path: entry.path().to_string_lossy().to_string(),
                        name,
                        abs_x_max,
                        abs_y_max,
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
