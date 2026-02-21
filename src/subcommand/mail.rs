use crate::error::{self, Error};
use log::error;
use mailparse::MailHeaderMap;
use snafu::ResultExt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(clap::Args)]
pub(crate) struct Mail;

impl Mail {
  pub(crate) fn run(self) -> Result<(), Error> {
    let mut message = Vec::new();
    io::stdin()
      .read_to_end(&mut message)
      .context(error::Stdin)?;

    save_to_maildir(&message)?;

    if let Err(e) = parse_and_reply(&message) {
      error!("failed to parse/reply: {e}");
    }

    Ok(())
  }
}

fn save_to_maildir(message: &[u8]) -> Result<(), Error> {
  let maildir = Path::new("/var/lib/lab/mail");

  for dir in ["cur", "new", "tmp"] {
    fs::create_dir_all(maildir.join(dir)).context(error::MaildirSave {
      path: maildir.display().to_string(),
    })?;
  }

  let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_nanos();

  let filename = format!("{timestamp}.lab.tulip.farm");
  let tmp_path = maildir.join("tmp").join(&filename);
  let new_path = maildir.join("new").join(&filename);

  fs::write(&tmp_path, message).context(error::MaildirSave {
    path: tmp_path.display().to_string(),
  })?;

  fs::rename(&tmp_path, &new_path).context(error::MaildirSave {
    path: new_path.display().to_string(),
  })?;

  Ok(())
}

fn extract_sender(parsed: &mailparse::ParsedMail) -> Option<String> {
  let headers = parsed.get_headers();

  headers
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
    })
}

fn extract_body(parsed: &mailparse::ParsedMail) -> Option<String> {
  if parsed.ctype.mimetype.starts_with("multipart/") {
    for subpart in &parsed.subparts {
      if let Some(body) = extract_body(subpart) {
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

fn reply_subject(subject: Option<&str>) -> String {
  match subject {
    None => String::from("Re:"),
    Some(s) if s.starts_with("Re:") || s.starts_with("re:") || s.starts_with("RE:") => {
      s.to_string()
    }
    Some(s) => format!("Re: {s}"),
  }
}

fn parse_and_reply(message: &[u8]) -> Result<(), Error> {
  let parsed = mailparse::parse_mail(message).context(error::MailParse)?;

  let sender = match extract_sender(&parsed) {
    Some(sender) => sender,
    None => return Ok(()),
  };

  let headers = parsed.get_headers();
  let subject = reply_subject(headers.get_first_value("Subject").as_deref());
  let body = extract_body(&parsed).unwrap_or_default();

  let mut reply = format!("From: root@tulip.farm\r\nTo: {sender}\r\nSubject: {subject}\r\n");

  if let Some(message_id) = headers.get_first_value("Message-ID") {
    reply.push_str(&format!("In-Reply-To: {message_id}\r\n"));

    let references = match headers.get_first_value("References") {
      Some(refs) => format!("{refs} {message_id}"),
      None => message_id,
    };
    reply.push_str(&format!("References: {references}\r\n"));
  }

  reply.push_str(&format!("\r\n{body}"));

  let mut child = Command::new("/run/wrappers/bin/sendmail")
    .arg("-t")
    .stdin(Stdio::piped())
    .spawn()
    .context(error::Io {
      path: String::from("/run/wrappers/bin/sendmail"),
    })?;

  child
    .stdin
    .take()
    .unwrap()
    .write_all(reply.as_bytes())
    .context(error::Io {
      path: String::from("sendmail stdin"),
    })?;

  let status = child.wait().context(error::Io {
    path: String::from("sendmail"),
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
    let message = b"From: foo@bar.com\r\nContent-Type: text/plain\r\n\r\nbaz";
    let parsed = mailparse::parse_mail(message).unwrap();
    assert_eq!(extract_body(&parsed).unwrap(), "baz");
  }

  #[test]
  fn extract_body_multipart_alternative() {
    let message = b"From: foo@bar.com\r\n\
            Content-Type: multipart/alternative; boundary=bound\r\n\r\n\
            --bound\r\n\
            Content-Type: text/plain\r\n\r\n\
            baz\r\n\
            --bound\r\n\
            Content-Type: text/html\r\n\r\n\
            <p>baz</p>\r\n\
            --bound--\r\n";
    let parsed = mailparse::parse_mail(message).unwrap();
    assert_eq!(extract_body(&parsed).unwrap(), "baz\r\n");
  }

  #[test]
  fn subject() {
    #[track_caller]
    fn case(input: Option<&str>, expected: &str) {
      assert_eq!(reply_subject(input), expected);
    }

    case(None, "Re:");
    case(Some("foo"), "Re: foo");
    case(Some("Re: foo"), "Re: foo");
    case(Some("re: foo"), "re: foo");
    case(Some("RE: foo"), "RE: foo");
  }

  #[test]
  fn empty_sender_no_reply() {
    let message = b"From: \r\nContent-Type: text/plain\r\n\r\nbaz";
    let parsed = mailparse::parse_mail(message).unwrap();
    assert!(extract_sender(&parsed).is_none());
  }
}
