#!/bin/sh

cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target i686-unknown-linux-musl

if [ $? -ne 0 ]; then
    echo "Build failed. Please check the errors above."
    exit 1
fi

cp target/x86_64-unknown-linux-musl/release/keyboard_test build/keyboard_test_x86_64
cp target/i686-unknown-linux-musl/release/keyboard_test build/keyboard_test_i686
