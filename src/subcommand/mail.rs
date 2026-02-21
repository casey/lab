use super::*;

use crate::message::Message;

const MAILDIR: &str = "/var/lib/lab/mail";

#[derive(clap::Args)]
pub(crate) struct Mail;

impl Mail {
  pub(crate) fn run(self) -> Result {
    let mut raw = Vec::new();
    io::stdin().read_to_end(&mut raw).context(error::Stdin)?;

    save_to_maildir(&raw)?;

    let message = Message::parse(&raw)?;

    reply(&message)?;

    Ok(())
  }
}

fn save_to_maildir(data: &[u8]) -> Result {
  let maildir = Path::new(MAILDIR);

  for dir in ["cur", "new", "tmp"] {
    fs::create_dir_all(maildir.join(dir)).context(error::MaildirSave {
      path: maildir.to_path_buf(),
    })?;
  }

  let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_nanos();

  let filename = format!("{timestamp}.lab.tulip.farm");
  let tmp_path = maildir.join("tmp").join(&filename);
  let new_path = maildir.join("new").join(&filename);

  fs::write(&tmp_path, data).context(error::MaildirSave {
    path: tmp_path.clone(),
  })?;

  fs::rename(&tmp_path, &new_path).context(error::MaildirSave { path: new_path })?;

  Ok(())
}

fn reply(message: &Message) -> Result {
  let reply = format!(
    "From: root@tulip.farm\r\nTo: {}\r\nSubject: {}\r\nIn-Reply-To: {}\r\nReferences: {}\r\n\r\n{}",
    message.sender,
    message.subject,
    message.message_id,
    message.references.join(" "),
    message.body,
  );

  save_to_maildir(reply.as_bytes())?;

  let mut child = Command::new("/run/wrappers/bin/sendmail")
    .arg("-t")
    .stdin(Stdio::piped())
    .spawn()
    .context(error::Io {
      path: PathBuf::from("/run/wrappers/bin/sendmail"),
    })?;

  child
    .stdin
    .take()
    .unwrap()
    .write_all(reply.as_bytes())
    .context(error::Io {
      path: PathBuf::from("sendmail stdin"),
    })?;

  let status = child.wait().context(error::Io {
    path: PathBuf::from("sendmail"),
  })?;

  if !status.success() {
    return Err(Error::Sendmail { status });
  }

  Ok(())
}
