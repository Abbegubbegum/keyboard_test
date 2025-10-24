use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
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

    /*
    println!("Found {} input devices:", devices.len());
    for (_, info) in &devices {
        println!("{}", info.name);
    }
     */

    for (mut dev, info) in devices {
        let tx_clone = tx.clone();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100)); // Allow some stagger time

            loop {
                match dev.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            match event.destructure() {
                                EventSummary::Key(_, code, 1) => {
                                    _ = tx_clone.send(AppEvent::Key {
                                        code,
                                        info: info.clone(),
                                    });
                                }
                                // Handle mouse movement events
                                EventSummary::RelativeAxis(_, rel_code, value) => {
                                    if rel_code == evdev::RelativeAxisCode::REL_X {
                                        // X movement
                                        _ = tx_clone.send(AppEvent::Mouse {
                                            x: value as i16,
                                            y: 0,
                                            info: info.clone(),
                                        });
                                    } else if rel_code == evdev::RelativeAxisCode::REL_Y {
                                        // Y movement
                                        _ = tx_clone.send(AppEvent::Mouse {
                                            x: 0,
                                            y: value as i16,
                                            info: info.clone(),
                                        });
                                    }
                                }
                                _ => {
                                    // println!(
                                    //     "Unhandled event from device {}: {:?}",
                                    //     info.name, event
                                    // );
                                    continue;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error fetching events from device {}: {}", info.name, e);
                        break; // Exit the loop on error
                    }
                }
            }
        });
    }

    let tx_clone = tx.clone();

    let _ = serial_touch::spawn_reader(tx_clone);

    Ok(())
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
