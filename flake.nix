{
  inputs = {
    claude-code.url = "github:sadjow/claude-code-nix";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { claude-code, home-manager, nixpkgs, ... }: {
    nixosConfigurations.lab = nixpkgs.lib.nixosSystem {
      specialArgs = { inherit claude-code; };
      modules = [
        { nixpkgs.hostPlatform = "x86_64-linux"; }
        home-manager.nixosModules.home-manager
        ./configuration.nix
      ];
    };
  };
}
