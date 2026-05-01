with (import <nixpkgs> {});

# shell for dev environment

mkShell {
  # build deps
  nativeBuildInputs = [
    cargo
    cmake
    pkg-config
  ];

  # runtime build deps
  buildInputs = [
    freetype
    expat
  ];
}
