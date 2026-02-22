{ claude-code, lib, pkgs, ... }:

let
  claude = claude-code.packages.${pkgs.stdenv.hostPlatform.system}.default;
  lab = pkgs.rustPlatform.buildRustPackage {
    pname = "lab";
    version = "0.0.0";
    src = lib.fileset.toSource {
      root = ./.;
      fileset = lib.fileset.unions [
        ./Cargo.toml
        ./Cargo.lock
        ./src
        ./tests
      ];
    };
    cargoLock.lockFile = ./Cargo.lock;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.systemd ];
  };
in
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

  environment.etc."opendmarc/opendmarc.conf".text = ''
    AuthservID tulip.farm
    TrustedAuthservIDs tulip.farm
    Socket local:/run/opendmarc/opendmarc.sock
    RejectFailures true
    RequiredHeaders true
    SPFIgnoreResults true
    SPFSelfValidate true
    Syslog true
  '';

  environment.variables.IS_SANDBOX = "1";

  environment.systemPackages = with pkgs; [
    btop
    clang
    claude
    delta
    dig
    eza
    gh
    git
    jq
    just
    neomutt
    neovim
    nix-search
    python3
    rustup
    tmux
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

  security.sudo.extraRules = [
    {
      users = [ "lab" ];
      commands = [
        {
          command = "ALL";
          options = [ "NOPASSWD" "SETENV" ];
        }
      ];
    }
  ];

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
      zones."tulip.farm.".data = builtins.readFile ./tulip.farm.zone;
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
      settings.master.lab = {
        type = "unix";
        privileged = true;
        chroot = false;
        maxproc = 1;
        command = "pipe";
        args = [
          "flags=RX"
          "user=lab:lab"
          "argv=${lab}/bin/lab mail --dir /var/lib/lab/mail --db /var/lib/lab/database.redb --claude ${claude}/bin/claude"
        ];
      };
      settings.main = {
        mailbox_transport = "lab";
        authorized_submit_users = [ "root" "lab" ];
        myhostname = "tulip.farm";
        mydomain = "tulip.farm";
        mydestination = [ "tulip.farm" "localhost" ];
        home_mailbox = "mail/";
        smtpd_tls_cert_file = "/var/lib/acme/tulip.farm/fullchain.pem";
        smtpd_tls_key_file = "/var/lib/acme/tulip.farm/key.pem";
        smtpd_tls_security_level = "encrypt";
        smtp_tls_security_level = "verify";
        smtpd_milters = "unix:/run/opendkim/opendkim.sock, unix:/run/opendmarc/opendmarc.sock";
        non_smtpd_milters = "unix:/run/opendkim/opendkim.sock";
        milter_default_action = "reject";
        smtpd_client_restrictions = "permit_mynetworks, reject_unknown_reverse_client_hostname";
        smtpd_sender_restrictions = "permit_mynetworks, check_sender_access hash:/var/lib/postfix/conf/sender_access, reject";
        smtpd_recipient_restrictions = "permit_mynetworks, reject_unauth_destination, check_recipient_access hash:/var/lib/postfix/conf/recipient_access, reject";
        smtpd_helo_required = "yes";
        smtpd_helo_restrictions = "permit_mynetworks, reject_invalid_helo_hostname, reject_non_fqdn_helo_hostname";
        disable_vrfy_command = "yes";
        strict_rfc821_envelopes = "yes";
        smtpd_tls_mandatory_protocols = ">=TLSv1.2";
        smtpd_tls_mandatory_ciphers = "high";
        tls_preempt_cipherlist = "yes";
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
      lab = {
        home = "/var/lib/lab";
        isSystemUser = true;
        group = "lab";
      };
      opendmarc = {
        isSystemUser = true;
        group = "opendmarc";
      };
      postfix.extraGroups = [ "opendkim" "opendmarc" "acme" ];
      root = {
        hashedPassword = "!";
        shell = pkgs.zsh;
        openssh.authorizedKeys.keys = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFbSqH7DNg3/USFtrLG183EVmL7VH7v+92qMbRvlOpSy rodarmor@odin"
          "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBPfEZoEAvyIpoy5oUiWdw6sHpIBgBKYfxd4cCSEIlHLVCW0e2WchLDRoMqRNb+Skl4AIlyF+vuaFLBaL+eJFmZs= rodarmor@soft-focus"
        ];
      };
    };

    groups.git = {};
    groups.lab = { members = [ "root" ]; };
    groups.opendmarc = {};
  };

  systemd.services.opendkim.serviceConfig.UMask = lib.mkForce "0007";

  systemd.services.opendmarc = {
    after = [ "network.target" "opendkim.service" ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "${pkgs.opendmarc}/bin/opendmarc -f -l -c /etc/opendmarc/opendmarc.conf";
      User = "opendmarc";
      Group = "opendmarc";
      RuntimeDirectory = "opendmarc";
      RuntimeDirectoryMode = "0750";
      UMask = "0007";
    };
  };

  home-manager.users.root = {
    home = {
      file.".claude/rules/lab.md".text = ''
        This NixOS server is a sandbox for agents.

        If a tool you would like to use is not installed, request permission to add it to the NixOS configuration.

        Do not force push.
      '';

      stateVersion = "26.05";
    };
  };

  system.stateVersion = "26.05";
}
