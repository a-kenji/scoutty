_: {
  perSystem =
    { pkgs, ... }:
    let
      scoutty = pkgs.callPackage ./scoutty.nix { };
    in
    {
      packages = {
        inherit scoutty;
        default = scoutty;
      };
    };
}
