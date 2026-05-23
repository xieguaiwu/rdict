{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell rec {
          packages = with pkgs; [
            cargo
            clippy
            rust-analyzer
            rustc
            rustfmt

            expat
            fontconfig
            freetype
            freetype.dev
            libGL
            pkg-config
            libX11
            libXcursor
            libXi
            libXrandr
            wayland
            libxkbcommon
          ];

          LD_LIBRARY_PATH = builtins.foldl' (a: b: "${a}:${b}/lib") "${pkgs.vulkan-loader}/lib" packages;
        };

        packages = {
          inherit ((pkgs.callPackage ./package.nix { })) default rdict rdict-telegram;
        };
      }
    );
}
