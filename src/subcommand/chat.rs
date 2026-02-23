use super::*;

use {
  base64::Engine,
  irc::client::{
    Client, ClientStream,
    data::Config,
    prelude::{Capability, Response, Sender},
  },
  irc::proto::{CapSubCommand, Command as IrcCommand},
  tokio_stream::StreamExt,
};

const SERVER: &str = "tulip.farm";
const PORT: u16 = 6697;
const NICK: &str = "root";
const PASSWORD_FILE: &str = "/root/secrets/ergo-password";
const SESSION_DIR: &str = "/root/sessions";
const ALLOWED_SENDER: &str = "rodarmor";
const CHATS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("chats");
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(clap::Args)]
pub(crate) struct Chat {
  #[arg(long)]
  db: Option<PathBuf>,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
}

impl Chat {
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

    loop {
      match self.run_connection(&password).await {
        Ok(()) => {}
        Err(e) => {
          log::error!("connection error: {e}");
          tokio::time::sleep(RECONNECT_DELAY).await;
        }
      }
    }
  }

  async fn run_connection(&self, password: &str) -> Result {
    let config = Config {
      server: Some(SERVER.to_string()),
      port: Some(PORT),
      nickname: Some(NICK.to_string()),
      use_tls: Some(true),
      ping_time: Some(30),
      ping_timeout: Some(20),
      ..Config::default()
    };

    let mut client = Client::from_config(config).await.context(error::Irc)?;
    let mut stream = client.stream().context(error::Irc)?;

    Self::sasl_auth(&client, &mut stream, password).await?;

    client.identify().context(error::Irc)?;

    while let Some(message) = stream.next().await.transpose().context(error::Irc)? {
      if let IrcCommand::PRIVMSG(ref target, ref text) = message.command {
        if !target.eq_ignore_ascii_case(NICK) {
          continue;
        }

        let sender = match message.source_nickname() {
          Some(nick) => nick.to_string(),
          None => continue,
        };

        if sender != ALLOWED_SENDER {
          continue;
        }

        let text = text.clone();
        let db = self.db.clone().unwrap_or_else(db_path);
        let claude = self.claude.clone();
        let irc_sender = client.sender();

        tokio::task::spawn_blocking(move || {
          match Self::handle_message(&db, &claude, &sender, &text) {
            Ok(response) => {
              if let Err(e) = Self::send_response(&irc_sender, &sender, &response) {
                log::error!("failed to send response: {e}");
              }
            }
            Err(e) => {
              log::error!("failed to handle message: {e}");
              let msg = format!("error: {e}").replace('\n', " | ");
              let _ = irc_sender.send_privmsg(&sender, &msg);
            }
          }
        })
        .await
        .context(error::TokioJoin)?;
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

  fn handle_message(db: &Path, claude: &Path, sender: &str, text: &str) -> Result<String> {
    let (session, resume) = Self::resolve_session(db, sender)?;
    invoke_agent(
      claude,
      Path::new(SESSION_DIR),
      &session,
      resume,
      text,
      Some(&format!(
        "You are chatting over IRC with {sender}. Your response should be plain text appropriate for IRC messages. Do not use markdown."
      )),
      true,
    )
  }

  fn resolve_session(db: &Path, sender: &str) -> Result<(String, bool)> {
    let db = redb::Database::create(db).context(error::DatabaseOpen { path: db })?;

    let read_txn = db.begin_read().context(error::DatabaseTransaction)?;
    let table = read_txn.open_table(CHATS);

    let existing = match table {
      Ok(table) => table
        .get(sender)
        .context(error::DatabaseStorage)?
        .map(|v| v.value().to_string()),
      Err(redb::TableError::TableDoesNotExist(_)) => None,
      Err(e) => return Err(e).context(error::DatabaseTable),
    };

    drop(read_txn);

    let resume = existing.is_some();
    let session = existing.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    if !resume {
      let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
      {
        let mut table = write_txn.open_table(CHATS).context(error::DatabaseTable)?;
        table
          .insert(sender, session.as_str())
          .context(error::DatabaseStorage)?;
      }
      write_txn.commit().context(error::DatabaseCommit)?;
    }

    Ok((session, resume))
  }

  fn send_response(sender: &Sender, target: &str, response: &str) -> Result {
    for line in response.lines() {
      let line = line.trim();

      if line.is_empty() {
        continue;
      }

      for chunk in split_utf8(line, 400) {
        sender.send_privmsg(target, chunk).context(error::Irc)?;
      }
    }
    Ok(())
  }
}

fn split_utf8(s: &str, max_bytes: usize) -> Vec<&str> {
  let mut chunks = Vec::new();
  let mut start = 0;

  while start < s.len() {
    let remaining = &s[start..];

    if remaining.len() <= max_bytes {
      chunks.push(remaining);
      break;
    }

    let mut end = start + max_bytes;

    while !s.is_char_boundary(end) {
      end -= 1;
    }

    chunks.push(&s[start..end]);
    start = end;
  }

  chunks
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn session_resolution() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("foo.redb");

    let (session1, resume1) = Chat::resolve_session(&db, "foo").unwrap();
    assert!(!resume1);

    let (session2, resume2) = Chat::resolve_session(&db, "foo").unwrap();
    assert!(resume2);
    assert_eq!(session1, session2);

    let (session3, resume3) = Chat::resolve_session(&db, "bar").unwrap();
    assert!(!resume3);
    assert_ne!(session1, session3);
  }

  #[test]
  fn split_utf8_short() {
    assert_eq!(split_utf8("foo", 400), vec!["foo"]);
  }

  #[test]
  fn split_utf8_exact() {
    let s = "a".repeat(400);
    assert_eq!(split_utf8(&s, 400), vec![s.as_str()]);
  }

  #[test]
  fn split_utf8_long() {
    let s = "a".repeat(801);
    let chunks = split_utf8(&s, 400);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 400);
    assert_eq!(chunks[1].len(), 400);
    assert_eq!(chunks[2].len(), 1);
  }

  #[test]
  fn split_utf8_multibyte() {
    let s = "\u{1F600}".repeat(101);
    let chunks = split_utf8(&s, 400);
    assert_eq!(chunks[0].len(), 400);
    for chunk in &chunks {
      assert!(chunk.len() <= 400);
    }
    let reassembled = chunks.join("");
    assert_eq!(reassembled, s);
  }
}
