{ claude-code, lib, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ];

  boot = {
    kernel.sysctl ={
      "net.ipv6.conf.enp0s3.accept_ra" = 1;
      "net.ipv6.conf.enp0s3.autoconf" = 1;
      "net.ipv6.conf.enp0s3.use_tempaddr" = 0;
    };
    loader.grub = {
      enable = true;
      device = "/dev/sda";
    };
  };

  environment.etc."claude-code/managed-settings.json" = {
    text = builtins.toJSON {
      permissions = {
        defaultMode = "bypassPermissions";
      };
    };
  };

  environment.variables.IS_SANDBOX = "1";

  environment.systemPackages = with pkgs; [
    btop
    delta
    dig
    clang
    claude-code.packages.${pkgs.stdenv.hostPlatform.system}.default
    eza
    git
    just
    neomutt
    neovim
    nix-search
    python3
    rustup
    zsh
  ];

  networking = {
    hostName = "lab";
    dhcpcd.IPv6rs = false;
    tempAddresses = "disabled";
    useDHCP = true;
    firewall = {
      enable = true;
      allowedTCPPorts = [ 22 25 53 80 443 ];
      allowedUDPPorts = [ 53 ];
    };
  };

  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    warn-dirty = false;
  };

  nixpkgs.config.allowUnfree = true;

  programs.zsh.enable = true;

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
      virtualHosts."tulip.farm" = {
        enableACME = true;
      };
    };

    nsd = {
      enable = true;
      interfaces = [ "0.0.0.0" "::" ];
      zones."tulip.farm.".data = ''
        $ORIGIN tulip.farm.
        $TTL 3600
        @  IN  SOA lab.rodarmor.com. casey.rodarmor.com. (
                 3          ; serial
                 3600       ; refresh
                 900        ; retry
                 604800     ; expire
                 3600       ; minimum
               )
        @       IN  NS   lab.rodarmor.com.
        @       IN  A    74.207.251.176
        @       IN  AAAA 2600:3c01::2000:41ff:fe8d:d2e1
        @       IN  MX   10 tulip.farm.
        @       IN  TXT  "v=spf1 a mx -all"
        _dmarc          IN  TXT  "v=DMARC1; p=reject; adkim=s; aspf=s"
        mail._domainkey IN  TXT  "v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDKwM6orWZCvbV13j8K7CY3UpQm2DvGL407YQ5GDhWat8FcC3Vh/dRAxBW5ED2Nc3leOsOPck+3xJNAEqQAAMlhQk9hHNynsQHXLPAHrhMFpPKkjh/fiyYcBEzEJPjefdxuafNKYJf0qsJUS1t668Lu6tsX6IApI1UiJGPgxCVUVQIDAQAB"
      '';
    };

    opendkim = {
      enable = true;
      selector = "mail";
      domains = "csl:tulip.farm";
      settings = {
        Mode = "sv";
        "On-BadSignature" = "reject";
        "On-NoSignature" = "reject";
        "On-KeyNotFound" = "reject";
        "On-DNSError" = "reject";
        "On-InternalError" = "reject";
        "On-Security" = "reject";
      };
    };

    openssh = {
      enable = true;
      settings = {
        PermitRootLogin = "prohibit-password";
        PasswordAuthentication = false;
      };
    };

    postfix = {
      enable = true;
      settings.main = {
        authorized_submit_users = [ "root" ];
        myhostname = "tulip.farm";
        mydomain = "tulip.farm";
        mydestination = [ "tulip.farm" "localhost" ];
        home_mailbox = "mail/";
        smtpd_tls_cert_file = "/var/lib/acme/tulip.farm/fullchain.pem";
        smtpd_tls_key_file = "/var/lib/acme/tulip.farm/key.pem";
        smtpd_tls_security_level = "encrypt";
        smtp_tls_security_level = "verify";
        smtpd_milters = "unix:/run/opendkim/opendkim.sock";
        non_smtpd_milters = "unix:/run/opendkim/opendkim.sock";
        milter_default_action = "accept";
        smtpd_sender_restrictions = "permit_mynetworks, check_sender_access hash:/var/lib/postfix/conf/sender_access, reject";
        smtpd_recipient_restrictions = "permit_mynetworks, reject_unauth_destination, check_recipient_access hash:/var/lib/postfix/conf/recipient_access, reject";
      };
      mapFiles = {
        sender_access = pkgs.writeText "sender_access" "casey@rodarmor.com OK\n";
        recipient_access = pkgs.writeText "recipient_access" "root@tulip.farm OK\n";
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
      postfix.extraGroups = [ "opendkim" "acme" ];
      root = {
        shell = pkgs.zsh;
        openssh.authorizedKeys.keys = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFbSqH7DNg3/USFtrLG183EVmL7VH7v+92qMbRvlOpSy rodarmor@odin"
        ];
      };
    };

    groups.git = {};
  };

  systemd.services.opendkim.serviceConfig.UMask = lib.mkForce "0007";

  system.stateVersion = "26.05";
}
