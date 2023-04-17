#!/bin/bash
# find a cargo.toml file in the current directory, if it does not exist error out
echo "========================="
echo "corrosionWM format script"
echo "========================="
echo

if [ ! -f Cargo.toml ]; then
    echo "No Cargo.toml file found in current directory"
    exit 1
fi

MAKEFLAGS="-j$(nproc)"

# run cargo fmt
cargo fmt

# clippy
cargo clippy

# ask if they want to cargo fix 
read -p "Do you want to run cargo fix? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]
then
    cargo fix
fi

echo "Done!"
