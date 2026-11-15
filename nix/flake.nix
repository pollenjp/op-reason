{
  description = "Development tools for op-reason (Rust toolchain, pwsh, fish and shell tools for the tests)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = lib.genAttrs systems;
    in
    {
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
              # The launcher tests run the launcher on PowerShell 7 (Linux)
              pkgs.powershell
              # The completion test runs fish
              pkgs.fish
              pkgs.jq
              pkgs.shellcheck
              pkgs.shfmt
              # flock, setsid and column for the black-box tests
              pkgs.util-linux
              pkgs.git
              pkgs.coreutils
            ];
            env = {
              POWERSHELL_TELEMETRY_OPTOUT = "1";
              POWERSHELL_UPDATECHECK = "Off";
              DOTNET_CLI_TELEMETRY_OPTOUT = "1";
              DOTNET_NOLOGO = "1";
            };
          };
        }
      );
    };
}
