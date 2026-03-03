use super::*;

#[derive(clap::Args)]
pub(crate) struct Task {
  #[arg(long)]
  name: String,
  #[arg(long)]
  prompt: PathBuf,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
  #[arg(long)]
  db: Option<PathBuf>,
  #[arg(long, default_value = "/run/wrappers/bin/sendmail")]
  sendmail: PathBuf,
  #[arg(long, default_value = "/root/mail")]
  dir: PathBuf,
  #[arg(long)]
  session: Option<String>,
  #[arg(long)]
  reset: bool,
}

impl Task {
  pub(crate) fn run(self) -> Result {
    let db_path = self.db.unwrap_or_else(db_path);

    if self.reset {
      let session = self.session.as_deref().expect("--reset requires --session");
      return reset_session(&db_path, session);
    }

    let body =
      fs::read_to_string(&self.prompt).context(error::FilesystemIo { path: &self.prompt })?;

    let (session, resume) = if let Some(ref name) = self.session {
      lookup_session(&db_path, name)?
    } else {
      (uuid::Uuid::now_v7().to_string(), false)
    };

    let response = invoke_agent(
      &self.claude,
      Path::new(SESSION_DIR),
      &session,
      resume,
      &body,
      None,
      false,
    )?;

    if let Some(ref name) = self.session
      && !resume
    {
      save_session(&db_path, name, &session)?;
    }

    let html = mail::Mail::markdown_to_html(&response);

    let message_id = format!("{}@tulip.farm", uuid::Uuid::now_v7());

    let db = redb::Database::create(&db_path).context(error::DatabaseOpen { path: &db_path })?;
    let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
    {
      let mut table = write_txn
        .open_table(mail::THREADS)
        .context(error::DatabaseTable)?;
      table
        .insert(message_id.as_str(), session.as_str())
        .context(error::DatabaseStorage)?;
    }
    write_txn.commit().context(error::DatabaseCommit)?;

    let mut subject = self.name.clone();
    if let Some(first) = subject.get_mut(..1) {
      first.make_ascii_uppercase();
    }

    let email = mail_builder::MessageBuilder::new()
      .from(("Root", "root@tulip.farm"))
      .to("casey@rodarmor.com")
      .subject(&subject)
      .message_id(message_id.as_str())
      .text_body(&response)
      .html_body(&html)
      .write_to_vec()
      .expect("writing to Vec failed");

    mail::Mail::save_to_maildir(&self.dir, &email)?;

    let envelope = lettre::address::Envelope::new(
      Some("root@tulip.farm".parse().unwrap()),
      vec!["casey@rodarmor.com".parse().unwrap()],
    )
    .unwrap();

    let transport = lettre::SendmailTransport::new_with_command(&self.sendmail);

    lettre::Transport::send_raw(&transport, &envelope, &email).context(error::Send)?;

    Ok(())
  }
}
