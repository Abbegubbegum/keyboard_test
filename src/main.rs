use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};

const KEYBOARD_LAYOUT: &[&[(&str, KeyCode)]] = &[
    &[
        ("ESC", KeyCode::KEY_ESC),
        ("F1", KeyCode::KEY_F1),
        ("F2", KeyCode::KEY_F2),
        ("F3", KeyCode::KEY_F3),
        ("F4", KeyCode::KEY_F4),
        ("F5", KeyCode::KEY_F5),
        ("F6", KeyCode::KEY_F6),
        ("F7", KeyCode::KEY_F7),
        ("F8", KeyCode::KEY_F8),
        ("F9", KeyCode::KEY_F9),
        ("F10", KeyCode::KEY_F10),
        ("F11", KeyCode::KEY_F11),
        ("F12", KeyCode::KEY_F12),
    ],
    &[
        ("`", KeyCode::KEY_GRAVE),
        ("1", KeyCode::KEY_1),
        ("2", KeyCode::KEY_2),
        ("3", KeyCode::KEY_3),
        ("4", KeyCode::KEY_4),
        ("5", KeyCode::KEY_5),
        ("6", KeyCode::KEY_6),
        ("7", KeyCode::KEY_7),
        ("8", KeyCode::KEY_8),
        ("9", KeyCode::KEY_9),
        ("0", KeyCode::KEY_0),
        ("+", KeyCode::KEY_MINUS),
        ("`", KeyCode::KEY_EQUAL),
        ("Backspace", KeyCode::KEY_BACKSPACE),
    ],
    &[
        ("Tab", KeyCode::KEY_TAB),
        ("Q", KeyCode::KEY_Q),
        ("W", KeyCode::KEY_W),
        ("E", KeyCode::KEY_E),
        ("R", KeyCode::KEY_R),
        ("T", KeyCode::KEY_T),
        ("Y", KeyCode::KEY_Y),
        ("U", KeyCode::KEY_U),
        ("I", KeyCode::KEY_I),
        ("O", KeyCode::KEY_O),
        ("P", KeyCode::KEY_P),
        ("Å", KeyCode::KEY_LEFTBRACE),
        ("^", KeyCode::KEY_RIGHTBRACE),
        ("Enter", KeyCode::KEY_ENTER),
    ],
    &[
        ("CapsLock", KeyCode::KEY_CAPSLOCK),
        ("A", KeyCode::KEY_A),
        ("S", KeyCode::KEY_S),
        ("D", KeyCode::KEY_D),
        ("F", KeyCode::KEY_F),
        ("G", KeyCode::KEY_G),
        ("H", KeyCode::KEY_H),
        ("J", KeyCode::KEY_J),
        ("K", KeyCode::KEY_K),
        ("L", KeyCode::KEY_L),
        ("Ö", KeyCode::KEY_SEMICOLON),
        ("Ä", KeyCode::KEY_APOSTROPHE),
        ("'", KeyCode::KEY_BACKSLASH),
    ],
    &[
        ("Shift", KeyCode::KEY_LEFTSHIFT),
        ("<", KeyCode::KEY_102ND),
        ("Z", KeyCode::KEY_Z),
        ("X", KeyCode::KEY_X),
        ("C", KeyCode::KEY_C),
        ("V", KeyCode::KEY_V),
        ("B", KeyCode::KEY_B),
        ("N", KeyCode::KEY_N),
        ("M", KeyCode::KEY_M),
        (",", KeyCode::KEY_COMMA),
        (".", KeyCode::KEY_DOT),
        ("-", KeyCode::KEY_SLASH),
        ("RShift", KeyCode::KEY_RIGHTSHIFT),
    ],
    &[
        ("LCtrl", KeyCode::KEY_LEFTCTRL),
        ("LWin", KeyCode::KEY_LEFTMETA),
        ("Alt", KeyCode::KEY_LEFTALT),
        ("Space", KeyCode::KEY_SPACE),
        ("Alt Gr", KeyCode::KEY_RIGHTALT),
        ("RWin", KeyCode::KEY_RIGHTMETA),
        ("RCtrl", KeyCode::KEY_RIGHTCTRL),
    ],
];

struct KeyboardDevice {
    path: String,
    name: String,
}

fn print_keyboard(pressed_keys: &HashMap<KeyCode, usize>) {
    // ANSI color codes
    const RESET: &str = "\x1b[0m";

    const COLORS: &[&str] = &[
        "\x1b[42m\x1b[30m", // Green
        "\x1b[43m\x1b[30m", // Orange
        "\x1b[41m\x1b[30m", // Red
        "\x1b[44m\x1b[30m", // Blue
    ];

    // Clear screen
    print!("\x1b[2J\x1b[H");
    io::stdout().flush().unwrap();

    for row in KEYBOARD_LAYOUT {
        for &key in row.iter() {
            let (key_str, keycode) = key;

            let press_count = pressed_keys.get(&keycode).copied().unwrap_or(0);

            if press_count > 0 {
                print!(
                    "{}{:^6}{}",
                    COLORS.get((press_count - 1) % 4).copied().unwrap_or(""),
                    key_str,
                    RESET
                );
            } else {
                print!("{:^6}", key_str);
            }
        }
        println!();
    }
    println!();
    println!("Press Ctrl+C to exit.");
}

fn get_keyboard_devices() -> Vec<KeyboardDevice> {
    let mut devices = Vec::new();

    let dir = fs::read_dir("/dev/input").expect("Failed to read /dev/input directory");

    for entry in dir.filter_map(Result::ok) {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.starts_with("event") {
            continue;
        }

        let path = format!("/dev/input/{file_name}");

        match Device::open(&path) {
            Ok(device) => {
                let name = device.name().unwrap_or("Unknown").to_string();

                // A way to check if the device is a keyboard is to check if supported keys include KEY_A
                if device
                    .supported_keys()
                    .map_or(false, |keys| keys.contains(KeyCode::KEY_A))
                {
                    devices.push(KeyboardDevice { path, name });
                }
            }
            Err(error) => {
                // Ignore devices that cannot be opened
                eprintln!("Could not open device {}: {}", path, error);
                continue;
            } // Skip devices that cannot be opened
        }
    }

    return devices;
}

fn main() {
    let devices = get_keyboard_devices();

    if devices.is_empty() {
        eprintln!("No keyboard devices found.");
        return;
    }

    let mut selected_device = devices.first().unwrap();

    println!("Found {} keyboard devices:", devices.len());
    for (i, device) in devices.iter().enumerate() {
        println!("{}: {} ({})", i + 1, device.name, device.path);
    }

    if devices.len() > 1 {
        println!(
            "Select device by entering its number (1-{}):",
            devices.len()
        );
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        let index: usize = input.trim().parse().unwrap_or(0);
        if index > 0 && index <= devices.len() {
            selected_device = &devices[index - 1];
        } else {
            eprintln!("Invalid selection, using the first device.");
        }
    }

    println!(
        "Using keyboard device: {} ({})",
        selected_device.name, selected_device.path
    );

    let mut keyboard_device = Device::open(&selected_device.path).expect("Failed to open device");

    let mut pressed_keys: HashMap<KeyCode, usize> = HashMap::new();

    print_keyboard(&pressed_keys);

    loop {
        for event in keyboard_device
            .fetch_events()
            .expect("Failed to fetch events")
        {
            match event.destructure() {
                EventSummary::Key(_, key_code, 1) => {
                    println!("Key pressed: {:?}", key_code);

                    *pressed_keys.entry(key_code).or_insert(0) += 1;

                    print_keyboard(&pressed_keys);
                }
                _ => {}
            }
        }
    }
}
