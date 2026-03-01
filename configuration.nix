{ claude-code, lib, pkgs, ... }:

let
  claude = claude-code.packages.${pkgs.stdenv.hostPlatform.system}.default;
  notebook-hook = pkgs.writeShellScript "notebook-post-receive" ''
    /run/wrappers/bin/sudo ${lab}/bin/lab note
  '';

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
    kernel.sysctl = {
      "net.ipv6.conf.enp0s3.accept_ra" = 1;
      "net.ipv6.conf.enp0s3.autoconf" = 1;
    };
    loader.grub = {
      enable = true;
      device = "/dev/sda";
    };
  };

  environment = {
    etc = {
      "claude-code/managed-settings.json".source = ./etc/claude.json;
      "opendmarc/opendmarc.conf".source = ./etc/opendmarc.conf;
    };

    variables.IS_SANDBOX = "1";

    systemPackages = with pkgs; [
      btop
      clang
      claude
      delta
      dig
      erdtree
      ergochat
      eza
      gh
      git
      jq
      just
      lab
      neomutt
      neovim
      nix-search
      openssl
      python3
      ripgrep
      rustup
      tmux
      tree
      zsh
    ];
  };

  home-manager.users.root = {
    home = {
      file.".claude/rules/lab.md".source = ./etc/lab.md;
      stateVersion = "26.05";
    };
  };

  networking = {
    hostName = "lab";
    useDHCP = false;
    interfaces.enp0s3.ipv4.addresses = [{
      address = "74.207.251.176";
      prefixLength = 24;
    }];
    defaultGateway = "74.207.251.1";
    nameservers = [ "45.33.56.79" "173.230.147.5" "74.207.242.5" ];
    tempAddresses = "disabled";
    firewall = {
      enable = true;
      allowedTCPPorts = [ 22 25 53 80 443 6697 ];
      allowedUDPPorts = [ 53 ];
    };
  };

  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    warn-dirty = false;
  };

  nixpkgs.config = {
    allowUnfree = true;
    permittedInsecurePackages = [
      "opendkim-2.11.0-Beta2"
    ];
  };

  programs.zsh.enable = true;

  time.timeZone = "America/Los_Angeles";

  security.acme = {
    acceptTerms = true;
    certs."tulip.farm".reloadServices = [ "ergo" "postfix" ];
    defaults.email = "casey@rodarmor.com";
  };

  security = {
    pam.services.sshd = {
      unixAuth = lib.mkForce true;
      allowNullPassword = true;
    };

    sudo.extraConfig = "Defaults closefrom_override";

    sudo.extraRules = [
      {
        users = [ "git" ];
        commands = [
          {
            command = "${lab}/bin/lab note";
            options = [ "NOPASSWD" ];
          }
        ];
      }
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

    journald.extraConfig = "Storage=persistent";

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
      virtualHosts."game.tulip.farm" = {
        forceSSL = true;
        enableACME = true;
        locations."/" = {
          proxyPass = "http://127.0.0.1:8080";
          proxyWebsockets = true;
        };
      };
      virtualHosts."tulip.farm" = {
        forceSSL = true;
        enableACME = true;
        root = ./www;
      };
    };

    nsd = {
      enable = true;
      interfaces = [ "0.0.0.0" "::" ];
      zones."tulip.farm.".data = builtins.readFile ./etc/tulip.farm.zone;
    };

    opendkim = {
      enable = true;
      selector = "mail";
      domains = "csl:tulip.farm";
      settings = {
        Mode = "sv";
        KeepAuthResults = "yes";
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
      extraConfig = ''
        Match User game
          ForceCommand /var/lib/game/game ssh
          PasswordAuthentication yes
          PermitEmptyPasswords yes
          DisableForwarding yes
          PermitUserRC no
          MaxSessions 2
          ClientAliveInterval 120
          ClientAliveCountMax 3
      '';
    };

    postfix = {
      enable = true;
      enableHeaderChecks = true;
      headerChecks = [
        { pattern = "/^Authentication-Results:.*tulip\\.farm/"; action = "IGNORE"; }
      ];
      settings.master.lab = {
        type = "unix";
        privileged = true;
        chroot = false;
        maxproc = 1;
        command = "pipe";
        args = [
          "flags=RX"
          "user=lab:lab"
          "argv=/run/wrappers/bin/sudo -i ${lab}/bin/lab mail --dir /root/mail --claude ${claude}/bin/claude"
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
      ergo = {
        isSystemUser = true;
        group = "ergo";
        extraGroups = [ "nginx" ];
      };
      game = {
        home = "/var/lib/game";
        isSystemUser = true;
        group = "game";
        hashedPassword = "";
        shell = pkgs.bash;
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

    groups = {
      ergo = {};
      game = {};
      git = {};
      lab = { members = [ "root" ]; };
      opendmarc = {};
    };
  };

  system = {
    activationScripts.notebook-hook = {
      deps = [ "users" ];
      text = ''
        REPO="/var/lib/forgejo/repositories/root/notebook.git"
        CONFIG="/var/lib/forgejo/custom/conf/app.ini"
        if [ -d "$REPO" ] && [ -f "$CONFIG" ]; then
          ${pkgs.su}/bin/su -s /bin/sh git -c \
            "${pkgs.forgejo}/bin/forgejo admin regenerate hooks --config=$CONFIG"
          mkdir -p "$REPO/hooks/post-receive.d"
          ln -sf ${notebook-hook} "$REPO/hooks/post-receive.d/notify"
          chown -h git:git "$REPO/hooks/post-receive.d/notify"
        fi
      '';
    };

    stateVersion = "26.05";
  };

  systemd = {
    tmpfiles.rules = [
      "d /root/secrets 0700 root root -"
    ];

    services = {
      chat = {
        after = [ "network.target" "ergo.service" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          ExecStart = "/run/wrappers/bin/sudo -i ${lab}/bin/lab chat --claude ${claude}/bin/claude";
          Restart = "always";
          RestartSec = 5;
        };
      };

      ergo = {
        after = [ "network.target" "acme-tulip.farm.service" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.ergochat}/bin/ergo run --conf ${./etc/ergo.yaml}";
          WorkingDirectory = "/var/lib/ergo";
          User = "ergo";
          Group = "ergo";
          StateDirectory = "ergo";
        };
      };

      game = {
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          ExecStart = "/var/lib/game/game --bind 127.0.0.1:8080";
          WorkingDirectory = "/var/lib/game";
          User = "game";
          Group = "game";
          StateDirectory = "game";
          Restart = "always";
          RestartSec = 5;
        };
      };

      gamemaster = {
        serviceConfig = {
          Type = "oneshot";
          ExecStart = "/run/wrappers/bin/sudo -i ${lab}/bin/lab gamemaster --claude ${claude}/bin/claude";
        };
      };

      notebook = {
        after = [ "network.target" ];
        serviceConfig = {
          ExecStart = "/run/wrappers/bin/sudo -C 4 -i ${lab}/bin/lab notebook --claude ${claude}/bin/claude";
          Restart = "always";
          RestartSec = 5;
        };
      };

      opendkim.serviceConfig.UMask = lib.mkForce "0007";

      opendmarc = {
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
    };

    timers.gamemaster = {
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = "*-*-* 04:00:00";
        Persistent = true;
      };
    };

    sockets.notebook = {
      listenDatagrams = [ "/run/notebook.sock" ];
      socketConfig.SocketMode = "0600";
      wantedBy = [ "sockets.target" ];
    };
  };

}
