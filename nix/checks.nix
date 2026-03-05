_: {
  perSystem =
    { self', ... }:
    {
      checks = {
        inherit (self'.packages) scoutty;
      };
    };
}
