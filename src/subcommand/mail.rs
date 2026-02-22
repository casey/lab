use super::*;

const THREADS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("threads");

#[derive(clap::Args)]
pub(crate) struct Mail {
  #[arg(long)]
  dir: PathBuf,
  #[arg(long, default_value = "/run/wrappers/bin/sendmail")]
  sendmail: PathBuf,
  #[arg(long)]
  db: PathBuf,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
  #[arg(long, default_value = "/root/sessions")]
  session_dir: PathBuf,
}

impl Mail {
  pub(crate) fn run(self) -> Result {
    let mut raw = Vec::new();
    io::stdin().read_to_end(&mut raw).context(error::Stdin)?;

    Self::save_to_maildir(&self.dir, &raw)?;

    let message = Message::parse(&raw)?;

    self.reply(&message)?;

    Ok(())
  }

  fn save_to_maildir(maildir: &Path, data: &[u8]) -> Result {
    for dir in ["cur", "new", "tmp"] {
      let path = maildir.join(dir);
      fs::create_dir_all(&path).context(error::FilesystemIo { path })?;
    }

    let timestamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();

    let filename = format!("{timestamp}.lab.tulip.farm");
    let tmp = maildir.join("tmp").join(&filename);
    let new = maildir.join("new").join(&filename);

    fs::write(&tmp, data).context(error::FilesystemIo { path: tmp.clone() })?;

    fs::rename(&tmp, &new).context(error::FilesystemIo { path: new })?;

    Ok(())
  }

  fn resolve_session(&self, message: &Message) -> Result<String> {
    let db = redb::Database::create(&self.db).context(error::DatabaseOpen {
      path: self.db.clone(),
    })?;

    let read_txn = db.begin_read().context(error::DatabaseTransaction)?;
    let table = read_txn.open_table(THREADS);

    let session = match table {
      Ok(table) => {
        let mut found = None;

        for id in message.references.iter().rev().skip(1) {
          if let Some(value) = table.get(id.as_str()).context(error::DatabaseStorage)? {
            found = Some(value.value().to_string());
            break;
          }
        }

        if found.is_none()
          && let Some(in_reply_to) = &message.in_reply_to
          && let Some(value) = table
            .get(in_reply_to.as_str())
            .context(error::DatabaseStorage)?
        {
          found = Some(value.value().to_string());
        }

        found
      }
      Err(redb::TableError::TableDoesNotExist(_)) => None,
      Err(e) => return Err(e).context(error::DatabaseTable),
    };

    drop(read_txn);

    let session = session.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
    {
      let mut table = write_txn
        .open_table(THREADS)
        .context(error::DatabaseTable)?;
      table
        .insert(message.message_id.as_str(), session.as_str())
        .context(error::DatabaseStorage)?;
    }
    write_txn.commit().context(error::DatabaseCommit)?;

    Ok(session)
  }

  fn invoke_agent(&self, session: &str, body: &str) -> Result<String> {
    let session_dir = self.session_dir.join(session);

    fs::create_dir_all(&session_dir).context(error::SessionDir {
      path: session_dir.clone(),
    })?;

    let output = Command::new(&self.claude)
      .arg("-p")
      .arg("--session-id")
      .arg(session)
      .arg("--append-system-prompt")
      .arg(format!("Your session ID is {session}."))
      .stdin(process::Stdio::piped())
      .stdout(process::Stdio::piped())
      .stderr(process::Stdio::piped())
      .current_dir(&session_dir)
      .spawn()
      .and_then(|mut child| {
        use io::Write;
        if let Some(mut stdin) = child.stdin.take() {
          stdin.write_all(body.as_bytes())?;
        }
        child.wait_with_output()
      })
      .context(error::AgentInvocation)?;

    if !output.status.success() {
      return Err(Error::AgentFailed {
        status: output.status,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
      });
    }

    String::from_utf8(output.stdout).context(error::AgentOutput)
  }

  fn markdown_to_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new(markdown);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
  }

  fn reply(&self, message: &Message) -> Result {
    let session = self.resolve_session(message)?;

    let response = self.invoke_agent(&session, &message.body)?;

    let html = Self::markdown_to_html(&response);

    let reply_id = format!("{}@tulip.farm", uuid::Uuid::now_v7());

    let db = redb::Database::create(&self.db).context(error::DatabaseOpen {
      path: self.db.clone(),
    })?;
    let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
    {
      let mut table = write_txn
        .open_table(THREADS)
        .context(error::DatabaseTable)?;
      table
        .insert(reply_id.as_str(), session.as_str())
        .context(error::DatabaseStorage)?;
    }
    write_txn.commit().context(error::DatabaseCommit)?;

    let reply = mail_builder::MessageBuilder::new()
      .from(("Root", "root@tulip.farm"))
      .to(message.sender.as_str())
      .subject(&message.subject)
      .message_id(reply_id.as_str())
      .in_reply_to(message.message_id.as_str())
      .references(
        message
          .references
          .iter()
          .map(|r| r.as_str())
          .collect::<Vec<&str>>(),
      )
      .text_body(&response)
      .html_body(&html)
      .write_to_vec()
      .expect("writing to Vec failed");

    Self::save_to_maildir(&self.dir, &reply)?;

    let envelope = lettre::address::Envelope::new(
      Some("root@tulip.farm".parse().unwrap()),
      vec![message.sender.parse().context(error::Address)?],
    )
    .unwrap();

    let transport = lettre::SendmailTransport::new_with_command(&self.sendmail);

    lettre::Transport::send_raw(&transport, &envelope, &reply).context(error::Send)?;

    Ok(())
  }
}
