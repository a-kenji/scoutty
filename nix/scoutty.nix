{
  lib,
  rustPlatform,
  installShellFiles,
}:
let
  root = ../.;
  cargoTOML = fromTOML (builtins.readFile (root + "/Cargo.toml"));
  inherit (cargoTOML.package) version name;
  pname = name;
  meta = import ./meta.nix { inherit lib; };
  fileset = lib.fileset.unions [
    (root + "/Cargo.toml")
    (root + "/Cargo.lock")
    (root + "/src")
  ];
in
rustPlatform.buildRustPackage {
  inherit pname version meta;

  nativeBuildInputs = [ installShellFiles ];

  src = lib.fileset.toSource {
    inherit root fileset;
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  postInstall = ''
    installShellCompletion --cmd scoutty \
      --bash <($out/bin/scoutty --completions bash) \
      --fish <($out/bin/scoutty --completions fish) \
      --zsh <($out/bin/scoutty --completions zsh)
  '';
}
