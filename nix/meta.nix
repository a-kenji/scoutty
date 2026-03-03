{ lib, ... }:
{
  description = "Terminal capability probe CLI";
  mainProgram = "scoutty";
  license = [ lib.licenses.mit ];
  platforms = lib.platforms.unix;
}
