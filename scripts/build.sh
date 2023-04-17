#!/bin/bash
# find a cargo.toml file in the current directory, if it does not exist error out
echo "========================"
echo "corrosionWM build script"
echo "========================"

if [ ! -f Cargo.toml ]; then
    echo "No Cargo.toml file found in current directory"
    exit 1
fi

MAKEFLAGS="-j$(nproc)"

# run cargo build --release
cargo build --release

# ask if they want to run cargo install
read -p "Do you want to run cargo install? [y/N] " -n 1 -r

if [[ $REPLY =~ ^[Yy]$ ]]
then
    cargo install --path .
fi

echo "Done!"
