{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem = _: {
    treefmt = {
      projectRootFile = ".git/config";
      programs.flake-edit.enable = true;
      programs.nixf-diagnose.enable = true;
      programs.nixfmt.enable = true;
      programs.rustfmt.enable = true;
      programs.taplo.enable = true;
    };
  };
}
