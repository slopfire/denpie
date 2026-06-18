{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  name = "denpie-dev";

  nativeBuildInputs = with pkgs; [
    # Rust toolchain manager; rust-toolchain.toml pins the channel/targets/components.
    rustup

    # Frontend build tool for the Yew/WASM dashboard.
    trunk

    # Protocol Buffers compiler used by prost-build.
    protobuf

    # Local task runner.
    just

    # SQLite CLI used by bootstrap checks.
    sqlite

    # C/C++ build tooling for native dependencies (libcaesium -> mozjpeg-sys, libwebp-sys, etc.).
    gcc
    gnumake
    cmake
    nasm
    perl
    pkg-config
    clang
    llvmPackages.libclang

    # Runtime crypto/image libraries that the Rust crates prefer to link against.
    openssl
    libwebp
    libpng
    libjpeg

    # Benchmark/utility helpers (optional but convenient).
    oha
    jq

    # Version control used by the autoupdater and install scripts.
    git
  ];

  shellHook = ''
    echo "Entering Denpie development shell..."

    # Make sure the Rust toolchain, wasm32 target, and components from
    # rust-toolchain.toml are installed. This is a no-op when everything
    # is already present.
    rustup show

    # Point build scripts to the libclang we provided.
    export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"

    # Help openssl-sys use the Nix-provided OpenSSL instead of building its own.
    export OPENSSL_NO_VENDOR=1

    # Ensure cargo finds pkg-config files for image libraries.
    export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libwebp}/lib/pkgconfig:${pkgs.libpng.dev}/lib/pkgconfig:${pkgs.libjpeg.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"

    echo "Rust: $(rustc --version)"
    echo "Cargo: $(cargo --version)"
    echo "Trunk: $(trunk --version)"
    echo "Run 'just' to see available development tasks."
  '';
}
