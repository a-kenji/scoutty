{
  perSystem =
    { pkgs, self', ... }:
    {
      checks.foot-test = pkgs.testers.nixosTest {
        name = "foot-test";

        nodes.machine =
          { pkgs, ... }:
          {
            environment.systemPackages = [
              self'.packages.scoutty
              pkgs.cage
              pkgs.foot
              pkgs.jq
            ];

            # Cage needs a seat; seatd provides one without a full desktop
            services.seatd.enable = true;

            users.users.test = {
              isNormalUser = true;
              extraGroups = [
                "video"
                "seat"
              ];
            };

            virtualisation.memorySize = 1024;
            hardware.graphics.enable = true;
          };

        testScript = ''
          machine.wait_for_unit("seatd.service")

          # Run scoutty inside foot inside cage (minimal Wayland compositor).
          # WLR_RENDERER=pixman avoids needing a GPU.
          machine.succeed(
              "su - test -c '"
              "WLR_RENDERER=pixman cage -- "
              "foot -e sh -c \""
              "scoutty --json > /tmp/scoutty.json 2>/tmp/scoutty.err"
              "\"' &"
          )

          machine.wait_for_file("/tmp/scoutty.json", timeout=30)

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
    };
}
