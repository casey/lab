{ ... }:
{
  imports = [ ./hardware-configuration.nix ];

  boot = {
    kernel.sysctl ={
      "net.ipv6.conf.enp0s3.accept_ra" = 1;
      "net.ipv6.conf.enp0s3.autoconf" = 1;
    };
    loader.grub = {
      enable = true;
      device = "/dev/sda";
    };
  };

  networking = {
    dhcpcd.IPv6rs = false;
    tempAddresses = "disabled";
    useDHCP = true;
    firewall = {
      enable = true;
      allowedTCPPorts = [ 22 ];
    };
  };

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  time.timeZone = "America/Los_Angeles";

  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "prohibit-password";
      PasswordAuthentication = false;
    };
  };

  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFbSqH7DNg3/USFtrLG183EVmL7VH7v+92qMbRvlOpSy rodarmor@odin"
  ];

  system.stateVersion = "26.05";
}
