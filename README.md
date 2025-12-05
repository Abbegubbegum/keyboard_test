To Build for Alpine:

1. Make sure you have the target installed 'rustup target add x86_64-unknown-linux-musl'
2. Run build command 'cargo build --release --target x86_64-unknown-linux-musl'
3. Find the binary at target/x86_64-unknown-linux-musl/release/input_device_test

For 32-bit:

1. 'rustup target add i686-unknown-linux-musl'
2. 'cargo build --release --target i686-unknown-linux-musl'
3. Find the binary at target/i686-unknown-linux-musl/release/input_device_test
