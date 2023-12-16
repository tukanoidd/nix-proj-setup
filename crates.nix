{...}: {
  perSystem = {
    pkgs,
    config,
    ...
  }: let
    crateName = "nix-proj-setup";
  in {
    nci = {
      projects."nix-proj-setup".path = ./.;
      crates.${crateName} = {};
    };
  };
}
