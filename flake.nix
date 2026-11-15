{
  description = "op-reason: a wrapper for the 1Password CLI that requires a reason and confines each approval to one call (WSL)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;

      # The executable is the WSL side; Windows uses the PowerShell entry from the release zip
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = lib.genAttrs systems;
      version = (lib.importTOML ./Cargo.toml).workspace.package.version;
    in
    {
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};

          # bin/op-reason, bin/op-reason-log and the fish completion for op-reason
          op-reason = pkgs.rustPlatform.buildRustPackage {
            pname = "op-reason";
            inherit version;
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
                ./launcher
                ./completions
                ./skill
                ./tests/cases
              ];
            };
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [
              "--package"
              "op-reason"
            ];
            cargoTestFlags = [ "--workspace" ];
            postInstall = ''
              ln -s op-reason "$out/bin/op-reason-log"
              mkdir -p "$out/share/fish/vendor_completions.d"
              "$out/bin/op-reason" --completion fish op-reason >"$out/share/fish/vendor_completions.d/op-reason.fish"
            '';
            meta = {
              description = "Wrapper for the 1Password CLI: every call gives a reason and gets its own approval";
              homepage = "https://github.com/pollenjp/op-reason";
              license = with lib.licenses; [
                mit
                asl20
              ];
              mainProgram = "op-reason";
              platforms = lib.platforms.linux;
            };
          };

          # The same plus bin/op (put it before the real op in PATH) and the fish completion for op
          op = pkgs.runCommand "op-reason-op-${version}" { meta = op-reason.meta // { mainProgram = "op"; }; } ''
            mkdir -p "$out/bin" "$out/share/fish/vendor_completions.d"
            for name in op op-reason op-reason-log; do
              ln -s ${op-reason}/bin/op-reason "$out/bin/$name"
            done
            ln -s ${op-reason}/share/fish/vendor_completions.d/op-reason.fish "$out/share/fish/vendor_completions.d/"
            ${op-reason}/bin/op-reason --completion fish op >"$out/share/fish/vendor_completions.d/op.fish"
          '';
        in
        {
          default = op-reason;
          inherit op-reason op;
        }
      );
    };
}
