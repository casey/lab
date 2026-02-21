use super::*;

use mailparse::MailHeaderMap;

pub(crate) struct Message {
  pub(crate) sender: String,
  pub(crate) subject: String,
  pub(crate) body: String,
  pub(crate) message_id: String,
  pub(crate) references: Vec<String>,
}

impl Message {
  pub(crate) fn parse(raw: &[u8]) -> Result<Self> {
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
      })
      .ok_or(Error::MissingSender)?;

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

    let message_id = headers
      .get_first_value("Message-ID")
      .ok_or(Error::MissingMessageId)?;

    let mut references = headers
      .get_first_value("References")
      .map(|refs| {
        refs
          .split_whitespace()
          .map(String::from)
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();

    references.push(message_id.clone());

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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extract_body_plain_text() {
    let raw = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
    let message = Message::parse(raw).unwrap();
    assert_eq!(message.body, "baz");
  }

  #[test]
  fn extract_body_multipart_alternative() {
    let raw = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\
            Content-Type: multipart/alternative; boundary=bound\r\n\r\n\
            --bound\r\n\
            Content-Type: text/plain\r\n\r\n\
            baz\r\n\
            --bound\r\n\
            Content-Type: text/html\r\n\r\n\
            <p>baz</p>\r\n\
            --bound--\r\n";
    let message = Message::parse(raw).unwrap();
    assert_eq!(message.body, "baz\r\n");
  }

  #[test]
  fn subject() {
    #[track_caller]
    fn case(input: &[u8], expected: &str) {
      let message = Message::parse(input).unwrap();
      assert_eq!(message.subject, expected);
    }

    case(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\n", "Re:");
    case(
      b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nSubject: foo\r\n\r\n",
      "Re: foo",
    );
    case(
      b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nSubject: Re: foo\r\n\r\n",
      "Re: foo",
    );
    case(
      b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nSubject: re: foo\r\n\r\n",
      "re: foo",
    );
    case(
      b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nSubject: RE: foo\r\n\r\n",
      "RE: foo",
    );
  }

  #[test]
  fn missing_sender() {
    let raw = b"From: \r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
    assert!(Message::parse(raw).is_err());
  }

  #[test]
  fn missing_message_id() {
    let raw = b"From: foo@bar.com\r\nContent-Type: text/plain\r\n\r\nbaz";
    assert!(Message::parse(raw).is_err());
  }

  #[test]
  fn references() {
    let raw = b"From: foo@bar.com\r\nMessage-ID: <baz@bar>\r\n\
                References: <foo@bar> <bar@bar>\r\n\r\n";
    let message = Message::parse(raw).unwrap();
    assert_eq!(message.references, ["<foo@bar>", "<bar@bar>", "<baz@bar>"]);
  }

  #[test]
  fn references_without_existing() {
    let raw = b"From: foo@bar.com\r\nMessage-ID: <baz@bar>\r\n\r\n";
    let message = Message::parse(raw).unwrap();
    assert_eq!(message.references, ["<baz@bar>"]);
  }
}
