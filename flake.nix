{
  inputs = {
    claude-code.url = "github:sadjow/claude-code-nix";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { claude-code, nixpkgs, ... }: {
    nixosConfigurations.lab = nixpkgs.lib.nixosSystem {
      specialArgs = { inherit claude-code; };
      modules = [
        { nixpkgs.hostPlatform = "x86_64-linux"; }
        ./configuration.nix
      ];
    };
  };
}
