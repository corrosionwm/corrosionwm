{
  description = "corrosionWM's flake, sets up a development on nix. Flake will eventually include modules and a package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
    in
    {
      devShells.x86_64-linux.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          pkg-config
          udev
          wayland
          seatd
          libinput
          libxkbcommon
          libdrm
          gdk-pixbuf
          wayland-scanner
          wayland-protocols
          mesa
          libglvnd
        ];
      };
    };
}
