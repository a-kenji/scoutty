_: {
  perSystem =
    { pkgs, self', ... }:
    let
      env = import ./env.nix { inherit pkgs; };
    in
    {
      devShells.default = pkgs.mkShellNoCC {
        name = "scoutty";
        inputsFrom = [ self'.packages.default ];
        packages = [
          pkgs.cargo
          pkgs.clippy
          pkgs.rust-analyzer
          pkgs.rustc
          pkgs.rustfmt
          self'.formatter.outPath
        ];
        inherit env;
      };
    };
}
