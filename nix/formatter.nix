{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem = _: {
    treefmt = {
      projectRootFile = ".git/config";
      programs.actionlint.enable = true;
      programs.flake-edit.enable = true;
      programs.nixf-diagnose.enable = true;
      programs.nixfmt.enable = true;
      programs.rustfmt.enable = true;
      programs.taplo.enable = true;
      programs.sizelint.enable = true;
    };
  };
}
