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
    let path = maildir.join(dir);
    fs::create_dir_all(&path).context(error::MaildirSave { path })?;
  }

  let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_nanos();

  let filename = format!("{timestamp}.lab.tulip.farm");
  let tmp = maildir.join("tmp").join(&filename);
  let new = maildir.join("new").join(&filename);

  fs::write(&tmp, data).context(error::MaildirSave { path: tmp.clone() })?;

  fs::rename(&tmp, &new).context(error::MaildirSave { path: new })?;

  Ok(())
}

fn reply(message: &Message) -> Result {
  let reply = mail_builder::MessageBuilder::new()
    .from("root@tulip.farm")
    .to(message.sender.as_str())
    .subject(&message.subject)
    .in_reply_to(message.message_id.as_str())
    .references(
      message
        .references
        .iter()
        .map(|r| r.as_str())
        .collect::<Vec<_>>(),
    )
    .text_body(&message.body)
    .write_to_vec()
    .expect("writing to Vec failed");

  save_to_maildir(&reply)?;

  let mut child = Command::new("/run/wrappers/bin/sendmail")
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

  let status = child.wait().context(error::SendmailInvoke)?;

  if !status.success() {
    return Err(Error::Sendmail { status });
  }

  Ok(())
}
