{
  description = "Commit GPT - Git commit message generator using OpenAI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
      in {
        packages = {
          commit-gpt = pkgs.callPackage ./default.nix {};
          default = pkgs.callPackage ./default.nix {};
        };
        formatter = pkgs.alejandra;
      }
    );
}
