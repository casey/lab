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
    hostName = "lab";
    dhcpcd.IPv6rs = false;
    tempAddresses = "disabled";
    useDHCP = true;
    firewall = {
      enable = true;
      allowedTCPPorts = [ 22 53 80 443 ];
      allowedUDPPorts = [ 53 ];
    };
  };

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  time.timeZone = "America/Los_Angeles";

  security.acme = {
    acceptTerms = true;
    defaults.email = "casey@rodarmor.com";
  };

  services = {
    forgejo = {
      enable = true;
      user = "git";
      group = "git";
      settings = {
        server = {
          DOMAIN = "lab.rodarmor.com";
          ROOT_URL = "https://lab.rodarmor.com/";
          HTTP_ADDR = "127.0.0.1";
          HTTP_PORT = 3000;
        };
        service = {
          DISABLE_REGISTRATION = true;
          REQUIRE_SIGNIN_VIEW = true;
          DEFAULT_ALLOW_CREATE_ORGANIZATION = false;
        };
        "service.explore" = {
          DISABLE_USERS_PAGE = true;
          DISABLE_ORGANIZATIONS_PAGE = true;
          DISABLE_CODE_PAGE = true;
        };
        repository = {
          DISABLE_HTTP_GIT = true;
          USE_COMPAT_SSH_URI = false;
          ENABLE_PUSH_CREATE_USER = true;
        };
        session.COOKIE_SECURE = true;
      };
    };

    nginx = {
      enable = true;
      virtualHosts."lab.rodarmor.com" = {
        forceSSL = true;
        enableACME = true;
        locations."/" = {
          proxyPass = "http://127.0.0.1:3000";
          proxyWebsockets = true;
        };
      };
    };

    nsd = {
      enable = true;
      interfaces = [ "74.207.251.176" "2600:3c01::2000:41ff:fe8d:d2e1" ];
      zones."tulip.farm.".data = ''
        $ORIGIN tulip.farm.
        $TTL 3600
        @  IN  SOA lab.rodarmor.com. casey.rodarmor.com. (
                 1          ; serial
                 3600       ; refresh
                 900        ; retry
                 604800     ; expire
                 3600       ; minimum
               )
        @  IN  NS   lab.rodarmor.com.
        @  IN  A    74.207.251.176
        @  IN  AAAA 2600:3c01::2000:41ff:fe8d:d2e1
      '';
    };

    openssh = {
      enable = true;
      settings = {
        PermitRootLogin = "prohibit-password";
        PasswordAuthentication = false;
      };
    };
  };

  users = {
    users = {
      git = {
        home = "/var/lib/forgejo";
        useDefaultShell = true;
        group = "git";
        isSystemUser = true;
      };
      root.openssh.authorizedKeys.keys = [
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFbSqH7DNg3/USFtrLG183EVmL7VH7v+92qMbRvlOpSy rodarmor@odin"
      ];
    };

    groups.git = {};
  };

  system.stateVersion = "26.05";
}
