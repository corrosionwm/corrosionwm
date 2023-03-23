# CorrosionWM
CorrosionWM is a blazing fast [Wayland compositor](https://wayland.freedesktop.org/) written in [Rust](https://www.rust-lang.org/).

## Features

[//]: # (I have no clue where this project is going, so here will be some placeholder items)

- [x] Can display terminal
- [ ] Keyboard works

## Contributing
To contribute your own code, start by forking this project and installing the necessary dependencies listed below.

After installing the dependencies, you can build the project with `cargo build`.

NOTE: A few packages will be downloaded and installed, so the first time may take significantly longer than subsequent builds.

The binary will be created in `./target/debug/`, named `corrosionwm`. You can now edit the source code and rebuild the project.
Test your changes by running the `corrosionwm` file.

When you have finished making all of your changes, commit them using [Git](https://git-scm.com/) and create a pull request.

### Dependencies

[//]: # (copied from the shell.nix file, please update later)

- cairo
- cargo
- dbus
- gdk-pixbuf
- libglvnd
- libinput
- libdrm
- libxkbcommon
- mesa
- pango
- seatd
- udev
- wayland
- wayland-protocols
- wayland-scanner
- wlroots
- xgboost
