{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell rec {
  buildInputs = with pkgs; [
    pkg-config
    dbus
    udev
    wayland
    seatd
    libinput
    libxkbcommon
    xgboost
    cairo
    libdrm
    gdk-pixbuf
    wayland-scanner
    pango
    wlroots
    wayland-protocols
    mesa
    libglvnd
  ];
}
