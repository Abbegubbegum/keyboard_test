use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossbeam_channel::Sender;
use crossterm::terminal;
use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::{fs, vec};
use std::{thread, time::Duration};

use crate::machine_detect::{ComputerModel, get_computer_model};
use crate::serial_touch;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FingerState {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub pressure: Option<i32>,
    pub major: Option<i32>,
    pub width: Option<i32>,
    pub pending: bool,
}

#[derive(Debug)]
pub enum TrackpadEvent {
    FingerUpdate { slot: usize, state: FingerState },
    FingerUp { slot: usize },
    Click { down: bool },
    FingerCount { count: usize },
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
    Trackpad {
        event: TrackpadEvent,
    },
}

pub struct TrackpadEventHandler {
    pub sender: Sender<AppEvent>,

    // last known state per finger id
    slots: Vec<Option<FingerState>>,
    current_slot: Option<usize>,

    // per-SYN aggregated pending fields
    pending_finger_up: Vec<usize>,
    pending_click: Option<bool>,
    pending_finger_count: Option<usize>,
}

impl TrackpadEventHandler {
    pub fn new(sender: Sender<AppEvent>) -> Self {
        Self {
            sender,
            slots: vec![None; 10],
            current_slot: None,

            pending_finger_up: vec![],
            pending_click: None,
            pending_finger_count: None,
        }
    }

    pub fn handle_event(&mut self, event: &EventSummary) {
        match *event {
            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_SLOT, value) => {
                self.current_slot = Some(value as usize);
            }
            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_TRACKING_ID, value) => {
                if let Some(slot) = self.current_slot {
                    if value < 0 {
                        // Finger lifted on this slot
                        if let Some(_old) = self.slots[slot].take() {
                            self.pending_finger_up.push(slot);
                        }
                    } else {
                        // New finger in this slot
                        self.slots[slot] = Some(FingerState {
                            x: None,
                            y: None,
                            pressure: None,
                            major: None,
                            width: None,
                            pending: true,
                        });
                    }
                }
            }

            // -------- ABSOLUTE MULTITOUCH --------
            EventSummary::AbsoluteAxis(_, abs_code, value) => {
                if let Some(slot) = self.current_slot {
                    if let Some(f) = self.slots[slot].as_mut() {
                        match abs_code {
                            AbsoluteAxisCode::ABS_MT_POSITION_X => {
                                f.x = Some(value);
                                f.pending = true;
                            }
                            AbsoluteAxisCode::ABS_MT_POSITION_Y => {
                                f.y = Some(value);
                                f.pending = true;
                            }
                            AbsoluteAxisCode::ABS_MT_PRESSURE => {
                                f.pressure = Some(value);
                                f.pending = true;
                            }
                            AbsoluteAxisCode::ABS_MT_TOUCH_MAJOR => {
                                f.major = Some(value);
                                f.pending = true;
                            }
                            AbsoluteAxisCode::ABS_MT_WIDTH_MAJOR => {
                                f.width = Some(value);
                                f.pending = true;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // -------- CLICK / BUTTONS --------
            EventSummary::Key(_, code, value) => match code {
                KeyCode::BTN_LEFT => {
                    self.pending_click = Some(value == 1);
                }
                KeyCode::BTN_TOOL_FINGER => {
                    self.pending_finger_count = Some(1);
                }
                KeyCode::BTN_TOOL_DOUBLETAP => {
                    self.pending_finger_count = Some(2);
                }
                KeyCode::BTN_TOOL_TRIPLETAP => {
                    self.pending_finger_count = Some(3);
                }
                KeyCode::BTN_TOOL_QUADTAP => {
                    self.pending_finger_count = Some(4);
                }
                _ => {}
            },

            // -------- SYN_REPORT = EMIT ONE EVENT --------
            EventSummary::Synchronization(..) => {
                self.flush_syn_report();
            }

            _ => {}
        }
    }

    fn flush_syn_report(&mut self) {
        // Click events (simple)
        if let Some(down) = self.pending_click.take() {
            let _ = self.sender.send(AppEvent::Trackpad {
                event: TrackpadEvent::Click { down },
            });
        }

        // Finger count
        if let Some(count) = self.pending_finger_count.take() {
            let _ = self.sender.send(AppEvent::Trackpad {
                event: TrackpadEvent::FingerCount { count },
            });
        }

        // Finger-Up
        for slot in self.pending_finger_up.drain(..) {
            let _ = self.sender.send(AppEvent::Trackpad {
                event: TrackpadEvent::FingerUp { slot },
            });
        }

        // Collect pending updates first to avoid borrowing self.slots mutably while it's immutably borrowed.
        let pending_updates: Vec<(usize, FingerState)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, f)| {
                f.as_ref().and_then(|fs| {
                    if fs.pending {
                        Some((slot, fs.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Send pending updates
        for (slot, state) in pending_updates {
            let _ = self.sender.send(AppEvent::Trackpad {
                event: TrackpadEvent::FingerUpdate { slot, state },
            });
        }

        // Now clear pending flags with a mutable iteration
        for maybe in self.slots.iter_mut() {
            if let Some(f) = maybe {
                f.pending = false;
            }
        }
    }
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

        // USB touchscreen state tracking
        let mut touchpad_x: u16 = 0;
        let mut touchpad_y: u16 = 0;
        let mut trackpad_handler = TrackpadEventHandler::new(tx.clone());

        loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for event in events {
                        // Mouse trackpad on the RS11
                        if info.name.contains("ETPS") {
                            trackpad_handler.handle_event(&event.destructure());
                            continue;
                        }

                        match event.destructure() {
                            EventSummary::Key(_, code, value) => {
                                // Handle BTN_TOUCH for USB touchscreens
                                if code == KeyCode::BTN_TOUCH {
                                    _ = tx.send(get_touch_event(
                                        touchpad_x,
                                        touchpad_y,
                                        value == 0,
                                    ));
                                } else if value == 1 {
                                    // Regular key press
                                    _ = tx.send(AppEvent::Key {
                                        code,
                                        info: info.clone(),
                                    });
                                }
                            }
                            // Handle USB touchscreen absolute axis events
                            EventSummary::AbsoluteAxis(_, abs_code, value) => match abs_code {
                                evdev::AbsoluteAxisCode::ABS_X => {
                                    touchpad_x = value as u16;
                                    _ = tx.send(get_touch_event(touchpad_x, touchpad_y, false));
                                }
                                evdev::AbsoluteAxisCode::ABS_Y => {
                                    touchpad_y = value as u16;
                                    _ = tx.send(get_touch_event(touchpad_x, touchpad_y, false));
                                }
                                _ => {}
                            },
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

fn get_touch_event(x: u16, y: u16, released: bool) -> AppEvent {
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
        }
    } else {
        AppEvent::Touch {
            x,
            y,
            timestamp,
            released,
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
