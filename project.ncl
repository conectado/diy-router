let inputs = import "./nickel.lock.ncl" in
let organist = inputs.organist in

organist.OrganistExpression
& {
  Schema,
  config | Schema
    = {
      shells = organist.shells.Rust
      & {
        build = {
          extraTargets = {
            "thumbv7m-none-eabi" = true,
            "thumbv7em-none-eabihf" = true,
          },
          components = {
            "llvm-tools-preview" = true,
            "rust-src" = true,
          },
        },
      },

      shells.build = {
        packages = {},
      },

      shells.dev = {
        packages = {
          # Usual extras you already had
          gcc-arm-embedded = organist.import_nix "nixpkgs#gcc-arm-embedded",
          usbutils = organist.import_nix "nixpkgs#usbutils",
          flip-link = organist.import_nix "nixpkgs#flip-link",
          qemu = organist.import_nix "nixpkgs#qemu",
          cargo-binutils = organist.import_nix "nixpkgs#cargo-binutils",
          openocd = organist.import_nix "nixpkgs#openocd",
        }

        # TODO: start openocd as a service
      },
    },
}
  | organist.modules.T
