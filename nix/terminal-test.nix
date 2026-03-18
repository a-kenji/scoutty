{
  perSystem =
    { pkgs, self', ... }:
    let
      terminals = {
        foot = {
          pkg = pkgs.foot;
          process = "foot";
        };
        alacritty = {
          pkg = pkgs.alacritty;
          process = "alacritty";
        };
        kitty = {
          pkg = pkgs.kitty;
          process = "kitty";
        };
      };

      mkTerminalTest =
        name:
        {
          pkg,
          process,
        }:
        pkgs.testers.nixosTest {
          name = "scoutty-${name}";

          nodes.machine =
            { pkgs, ... }:
            {
              environment.systemPackages = [
                self'.packages.scoutty
                pkgs.jq
              ];

              users.users.alice = {
                isNormalUser = true;
                extraGroups = [
                  "video"
                  "seat"
                ];
              };

              services.cage = {
                enable = true;
                user = "alice";
                program = "${pkg}/bin/${process}";
              };

              hardware.graphics.enable = true;
              virtualisation.qemu.options = [ "-vga none -device virtio-gpu-pci" ];
              virtualisation.memorySize = 2048;
            };

          testScript = ''
            start_all()
            machine.wait_for_file("/run/user/1000/wayland-0.lock")
            machine.wait_until_succeeds("pgrep ${process}")

            # Wait for the shell inside the terminal to be ready
            machine.sleep(3)

            # Use a sentinel file so we know scoutty has finished writing
            machine.send_chars("scoutty --json > /tmp/scoutty.json 2>/tmp/scoutty.err; touch /tmp/scoutty.done\n")
            machine.wait_for_file("/tmp/scoutty.done", timeout=30)

            # Validate JSON structure
            machine.succeed("jq .probes /tmp/scoutty.json")
            machine.succeed("jq -e '.probes.identity' /tmp/scoutty.json")

            # DA1 must always get a response from any real terminal
            machine.succeed(
                "jq -e '.probes.identity[] | select(.name == \"da1\") | .status == \"supported\"' "
                "/tmp/scoutty.json"
            )
          '';
        };
    in
    {
      checks = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (
        pkgs.lib.mapAttrs' (
          name: cfg: pkgs.lib.nameValuePair "scoutty-${name}" (mkTerminalTest name cfg)
        ) terminals
      );
    };
}
