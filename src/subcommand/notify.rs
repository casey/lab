use super::*;

use {
  base64::Engine,
  irc::client::{
    Client, ClientStream,
    data::Config,
    prelude::{Capability, Response},
  },
  irc::proto::{CapSubCommand, Command as IrcCommand},
  tokio_stream::StreamExt,
};

const SERVER: &str = "tulip.farm";
const PORT: u16 = 6697;
const NICK: &str = "system";
const PASSWORD_FILE: &str = "/root/secrets/ergo-password";
const TARGET: &str = "rodarmor";

#[derive(clap::Args)]
pub(crate) struct Notify {
  message: String,
}

impl Notify {
  pub(crate) fn run(self) -> Result {
    let rt = tokio::runtime::Runtime::new().context(error::TokioRuntime)?;
    rt.block_on(self.run_async())
  }

  async fn run_async(&self) -> Result {
    let password = fs::read_to_string(PASSWORD_FILE)
      .context(error::PasswordFile {
        path: Path::new(PASSWORD_FILE),
      })?
      .trim()
      .to_string();

    let config = Config {
      server: Some(SERVER.to_string()),
      port: Some(PORT),
      nickname: Some(NICK.to_string()),
      use_tls: Some(true),
      ..Config::default()
    };

    let mut client = Client::from_config(config).await.context(error::Irc)?;
    let mut stream = client.stream().context(error::Irc)?;

    Self::sasl_auth(&client, &mut stream, &password).await?;

    client.identify().context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::Response(Response::RPL_WELCOME, _) = &message.command {
        break;
      }
    }

    let sender = client.sender();
    sender
      .send_privmsg(TARGET, &self.message)
      .context(error::Irc)?;
    sender.send_quit("").context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::ERROR(_) = &message.command {
        break;
      }
    }

    Ok(())
  }

  async fn sasl_auth(client: &Client, stream: &mut ClientStream, password: &str) -> Result {
    client
      .send_cap_req(&[Capability::Sasl])
      .context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::CAP(_, CapSubCommand::ACK, _, _) = &message.command {
        break;
      }
    }

    client.send_sasl_plain().context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::AUTHENTICATE(ref param) = message.command
        && param == "+"
      {
        break;
      }
    }

    let credentials = format!("\0{NICK}\0{password}");
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    client.send_sasl(&encoded).context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::Response(ref response, _) = message.command {
        if *response == Response::RPL_SASLSUCCESS {
          return Ok(());
        }
        if *response == Response::ERR_SASLFAIL {
          return Err(Error::IrcProtocol {
            message: "SASL authentication failed".to_string(),
          });
        }
      }
    }

    Err(Error::IrcProtocol {
      message: "connection closed during SASL authentication".to_string(),
    })
  }
}
