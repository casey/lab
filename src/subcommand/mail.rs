use super::*;

#[derive(clap::Args)]
pub(crate) struct Mail {
  #[arg(long)]
  dir: PathBuf,
  #[arg(long, default_value = "/run/wrappers/bin/sendmail")]
  sendmail: PathBuf,
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

  fn reply(&self, message: &Message) -> Result {
    let reply = mail_builder::MessageBuilder::new()
      .from(("Root", "root@tulip.farm"))
      .to(message.sender.as_str())
      .subject(&message.subject)
      .in_reply_to(message.message_id.as_str())
      .references(
        message
          .references
          .iter()
          .map(|r| r.as_str())
          .collect::<Vec<&str>>(),
      )
      .text_body(&message.body)
      .write_to_vec()
      .expect("writing to Vec failed");

    Self::save_to_maildir(&self.dir, &reply)?;

    let mut child = Command::new(&self.sendmail)
      .arg("-t")
      .stdin(Stdio::piped())
      .spawn()
      .context(error::SendmailInvoke)?;

    child
      .stdin
      .take()
      .unwrap()
      .write_all(&reply)
      .context(error::SendmailStdin)?;

    let status = child.wait().context(error::SendmailWait)?;

    if !status.success() {
      return Err(Error::Sendmail { status });
    }

    Ok(())
  }
}
