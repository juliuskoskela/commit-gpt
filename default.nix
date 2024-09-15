{pkgs ? import <nixpkgs> {}}:
pkgs.rustPlatform.buildRustPackage {
  pname = "commit-gpt";
  version = "0.1.0";

  src = pkgs.lib.cleanSource ./.;

  cargoLock.lockFile = ./Cargo.lock;

  buildInputs = [pkgs.pkg-config pkgs.libgit2];

  meta = {
    description = "A tool to generate Git commit messages using OpenAI.";
    homepage = "https://github.com/yourusername/commit-gpt";
    license = "MIT";
    maintainers = with pkgs.lib.maintainers; [juliuskoskela];
  };
}
