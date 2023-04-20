<p align="center">
 <h1 align="center">corrosionWM</h1>
 <h3 align="center">an aesthetic-oriented and speedy wayland compositor</h3>
</p>
  <p align="center">
    <img src="https://img.shields.io/github/languages/top/corrosionwm/corrosionwm?style=for-the-badge"/>
    <img src="https://img.shields.io/github/commit-activity/m/corrosionwm/corrosionwm?style=for-the-badge"/>
    <img src="https://img.shields.io/github/license/corrosionwm/corrosionwm?style=for-the-badge"/>
    <img src="https://img.shields.io/github/issues/corrosionwm/corrosionwm?style=for-the-badge"/>
  </p>

## About

CorrosionWM is a blazing fast [Wayland compositor](https://wayland.freedesktop.org/) written in [Rust](https://www.rust-lang.org/).

Join our [Discord](https://discord.gg/6sRvfeaNbQ)!

We also have a [Matrix](https://matrix.to/#/#corrosionwm:matrix.org)!

## Features

- [x] Can display simple applications
- [ ] Can launch from display manager (WIP)
- [ ] NVIDIA support (will be buggy due to [this](https://arewewaylandyet.com/) (see "NVIDIA"), and will likely need nouveau drivers)
- [ ] Can launch popups
- [x] Can launch from TTY (kind of)

## Contributing

**NOTE:** A more structured guide will be written soon!

To contribute your own code, start by forking this project and installing the necessary dependencies listed below.

After installing the dependencies, you can build the project with `cargo build --release`.

NOTE: A few packages will be downloaded and installed, so the first time may take significantly longer than subsequent builds.

The binary will be created in `./target/release/`, named `corrosionwm`.

> 💡 You can also use `cargo run --release` to run the project!

You can now edit the source code and rebuild the project.
Test your changes by running the `corrosionwm` file.

When you have finished making all of your changes, commit them using [Git](https://git-scm.com/) and create a pull request.

## Dependencies

[//]: # (add for other distros)

### Ubuntu

```bash
sudo apt install libudev-dev libdbus-1-dev pkg-config libsystemd-dev libwayland-dev libseat-dev xcb libinput-dev libxkbcommon-dev libglvnd-dev
```

### Arch

***NOTE! This has not been tested yet! If we missed one please let us know!***

```bash
sudo pacman -Syu wayland wayland-protocols libinput libxkbcommon libglvnd seatd dbus-glib 
```

### NixOS or systems with the nix package manager installed

This repository is a flake, and you can easily make a development shell environment with the dependenices installed by executing the following command in the cloned repository directory (btw, you need to [enable flakes](https://nixos.wiki/wiki/Flakes#Enable_flakes) for this to work):

```bash
nix develop
```
