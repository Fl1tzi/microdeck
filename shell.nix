with (import <nixpkgs> {});

# shell for dev environment

mkShell {
  # build deps
  nativeBuildInputs = [
    pkgs.cmake
    pkgs.pkg-config
  ];

  # runtime build deps
  buildInputs = [
    pkgs.udev
    pkgs.freetype
    pkgs.expat
  ];
}
