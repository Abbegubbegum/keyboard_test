use crate::{
    event_handler::AppEvent,
    machine_detect::{ComputerModel, get_computer_model},
};

use std::{thread, time::Duration};

use crossbeam_channel::Sender;

use color_eyre::{Result, eyre::eyre};

struct Decoder {
    state: u8,
    y_hi: u8,
    y_lo: u8,
    x_hi: u8,

    is_touching: bool,
}

impl Decoder {
    fn new() -> Self {
        Decoder {
            state: 0,
            y_hi: 0,
            y_lo: 0,
            x_hi: 0,
            is_touching: false,
        }
    }

    fn feed(&mut self, byte: u8) -> Option<AppEvent> {
        match self.state {
            0 => {
                if byte == 0xFF {
                    self.state = 1;
                } else if byte == 0xBF {
                    self.is_touching = !self.is_touching;
                    self.state = 1;
                }
            }
            1 => {
                self.y_hi = byte;
                self.state = 2;
            }
            2 => {
                self.y_lo = byte;
                self.state = 3;
            }
            3 => {
                self.x_hi = byte;
                self.state = 4;
            }
            4 => {
                let x_lo = byte;
                let x = ((self.x_hi as u16) << 7) | (x_lo as u16);
                let y = ((self.y_hi as u16) << 7) | (self.y_lo as u16);
                self.state = 0;
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                return Some(AppEvent::Touch {
                    x,
                    y,
                    timestamp,
                    released: !self.is_touching,
                });
            }
            _ => {
                self.state = 0; // Reset on unexpected state
            }
        }
        None
    }
}

pub fn spawn_reader(tx: Sender<AppEvent>) -> Result<std::thread::JoinHandle<()>> {
    if get_computer_model() != ComputerModel::DatorBBFält {
        return Err(eyre!(
            "serial touch reader can only be spawned on DatorBärbarFält model"
        ));
    }

    let _tx = tx.clone();

    let path = "/dev/ttyS3";
    let baud = 19200;
    let timeout_ms = 1000;

    let handle = thread::spawn(move || {
        let mut attempts = 0usize;
        loop {
            match serialport::new(path, baud)
                .timeout(Duration::from_millis(timeout_ms))
                .data_bits(serialport::DataBits::Eight)
                .parity(serialport::Parity::None)
                .stop_bits(serialport::StopBits::One)
                .flow_control(serialport::FlowControl::None)
                .open()
            {
                Ok(mut port) => {
                    let mut decoder = Decoder::new();
                    let mut buffer = [0u8; 256];
                    loop {
                        match port.read(&mut buffer) {
                            Ok(n) if n > 0 => {
                                for &byte in &buffer[..n] {
                                    if let Some(event) = decoder.feed(byte) {
                                        let _ = _tx.send(event);
                                    }
                                }
                            }
                            Ok(_) => {
                                thread::sleep(Duration::from_millis(2));
                            }
                            Err(e) => {
                                if e.kind() != std::io::ErrorKind::TimedOut {
                                    eprintln!("Error reading from serial port {}: {}", path, e);
                                    break; // Exit inner loop to attempt reopening
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    attempts += 1;
                    if attempts % 10 == 0 {
                        eprintln!("Failed to open serial port {}: {}. Retrying...", path, e);
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });

    Ok(handle)
}
