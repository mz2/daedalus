{
  description = "Daedalus — orchestrate & monitor agentic tools in isolated sandboxes";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          inherit (pkgs) lib stdenv;

          # Libraries gpui's Linux platform layer dlopen()s at run time (by soname),
          # plus what its build links against. The dlopen set is why a bare
          # `cargo run --features gpui` panics with `NoWaylandLib` outside this shell.
          gpuiLinuxLibs = with pkgs; [
            wayland
            libxkbcommon
            vulkan-loader
            libGL
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            xorg.libXext
            xorg.libXfixes
            xorg.libxcb
            fontconfig
            freetype
          ];

          commonTools = with pkgs; [
            pkg-config
            cmake
            nasm # rav1e/ravif (image deps pulled by gpui)
            # Rust toolchain (drop these if you prefer your own rustup).
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            # Session multiplexing used by the backends.
            zellij
          ];
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = commonTools;
            buildInputs = lib.optionals stdenv.isLinux gpuiLinuxLibs;

            # gpui dlopen()s wayland/vulkan/xkbcommon/libGL by name → expose them, plus the
            # host GPU driver that NixOS installs at /run/opengl-driver.
            LD_LIBRARY_PATH = lib.optionalString stdenv.isLinux (
              "${lib.makeLibraryPath gpuiLinuxLibs}:/run/opengl-driver/lib"
            );

            shellHook = ''
              echo "daedalus dev shell ready ($(rustc --version 2>/dev/null || echo 'rust: use your rustup'))."
              echo "  headless : DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop"
              echo "  window   : DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop --features gpui"
              echo "  test     : cargo test --workspace     lint: cargo clippy --workspace --all-targets -- -D warnings"
            '';
          };
        }
      );
    };
}
