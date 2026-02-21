use super::*;

const MAILDIR: &str = "/var/lib/lab/mail";

#[derive(clap::Args)]
pub(crate) struct Mail;

impl Mail {
  pub(crate) fn run(self) -> Result {
    let mut raw = Vec::new();
    io::stdin().read_to_end(&mut raw).context(error::Stdin)?;

    let message = Message::parse(&raw)?;

    save_to_maildir(&raw)?;

    if let Err(e) = reply(&message) {
      log::error!("failed to reply: {e}");
    }

    Ok(())
  }
}

struct Message {
  sender: Option<String>,
  subject: String,
  body: String,
  message_id: Option<String>,
  references: Option<String>,
}

impl Message {
  fn parse(raw: &[u8]) -> Result<Self> {
    let parsed = mailparse::parse_mail(raw).context(error::MailParse)?;
    let headers = parsed.get_headers();

    let sender = headers
      .get_first_value("Reply-To")
      .or_else(|| headers.get_first_value("From"))
      .and_then(|value| {
        let addr = if let Some(start) = value.find('<') {
          let end = value.find('>')?;
          value[start + 1..end].to_string()
        } else {
          value.trim().to_string()
        };

        if addr.is_empty() { None } else { Some(addr) }
      });

    let subject = {
      let raw = headers.get_first_value("Subject");
      match raw.as_deref() {
        None => String::from("Re:"),
        Some(s) if s.starts_with("Re:") || s.starts_with("re:") || s.starts_with("RE:") => {
          s.to_string()
        }
        Some(s) => format!("Re: {s}"),
      }
    };

    let body = Self::extract_body(&parsed).unwrap_or_default();

    let message_id = headers.get_first_value("Message-ID");

    let references = message_id
      .as_ref()
      .map(|id| match headers.get_first_value("References") {
        Some(refs) => format!("{refs} {id}"),
        None => id.clone(),
      });

    Ok(Self {
      sender,
      subject,
      body,
      message_id,
      references,
    })
  }

  fn extract_body(parsed: &mailparse::ParsedMail) -> Option<String> {
    if parsed.ctype.mimetype.starts_with("multipart/") {
      for subpart in &parsed.subparts {
        if let Some(body) = Self::extract_body(subpart) {
          return Some(body);
        }
      }
      None
    } else if parsed.ctype.mimetype == "text/plain" {
      parsed.get_body().ok()
    } else {
      None
    }
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
  let sender = match &message.sender {
    Some(sender) => sender,
    None => return Ok(()),
  };

  let mut reply = format!(
    "From: root@tulip.farm\r\nTo: {sender}\r\nSubject: {}\r\n",
    message.subject
  );

  if let Some(message_id) = &message.message_id {
    reply.push_str(&format!("In-Reply-To: {message_id}\r\n"));
  }

  if let Some(references) = &message.references {
    reply.push_str(&format!("References: {references}\r\n"));
  }

  reply.push_str(&format!("\r\n{}", message.body));

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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extract_body_plain_text() {
    let parsed =
      mailparse::parse_mail(b"From: foo@bar.com\r\nContent-Type: text/plain\r\n\r\nbaz").unwrap();
    assert_eq!(Message::extract_body(&parsed).unwrap(), "baz");
  }

  #[test]
  fn extract_body_multipart_alternative() {
    let parsed = mailparse::parse_mail(
      b"From: foo@bar.com\r\n\
            Content-Type: multipart/alternative; boundary=bound\r\n\r\n\
            --bound\r\n\
            Content-Type: text/plain\r\n\r\n\
            baz\r\n\
            --bound\r\n\
            Content-Type: text/html\r\n\r\n\
            <p>baz</p>\r\n\
            --bound--\r\n",
    )
    .unwrap();
    assert_eq!(Message::extract_body(&parsed).unwrap(), "baz\r\n");
  }

  #[test]
  fn subject() {
    #[track_caller]
    fn case(input: &[u8], expected: &str) {
      let message = Message::parse(input).unwrap();
      assert_eq!(message.subject, expected);
    }

    case(b"From: foo@bar.com\r\n\r\n", "Re:");
    case(b"From: foo@bar.com\r\nSubject: foo\r\n\r\n", "Re: foo");
    case(b"From: foo@bar.com\r\nSubject: Re: foo\r\n\r\n", "Re: foo");
    case(b"From: foo@bar.com\r\nSubject: re: foo\r\n\r\n", "re: foo");
    case(b"From: foo@bar.com\r\nSubject: RE: foo\r\n\r\n", "RE: foo");
  }

  #[test]
  fn empty_sender_no_reply() {
    let message = Message::parse(b"From: \r\nContent-Type: text/plain\r\n\r\nbaz").unwrap();
    assert!(message.sender.is_none());
  }
}
