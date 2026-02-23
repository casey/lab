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
        "You are chatting over IRC with {sender}."
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
    let response = markdown_to_plaintext(response);

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

enum ListKind {
  Ordered(u64),
  Unordered,
}

struct TableState {
  rows: Vec<Vec<String>>,
  current_row: Vec<String>,
  current_cell: String,
}

fn markdown_to_plaintext(markdown: &str) -> String {
  use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

  let mut options = Options::empty();
  options.insert(Options::ENABLE_TABLES);
  options.insert(Options::ENABLE_STRIKETHROUGH);
  options.insert(Options::ENABLE_SUPERSCRIPT);
  options.insert(Options::ENABLE_SUBSCRIPT);

  let parser = Parser::new_ext(markdown, options);

  let mut output = String::new();
  let mut list_stack: Vec<ListKind> = Vec::new();
  let mut in_heading = false;
  let mut heading_text = String::new();
  let mut in_code_block = false;
  let mut link_url = String::new();
  let mut link_text = String::new();
  let mut in_link = false;
  let mut image_url = String::new();
  let mut image_text = String::new();
  let mut in_image = false;
  let mut blockquote_depth: usize = 0;
  let mut table_state: Option<TableState> = None;
  let mut at_line_start = true;

  let blockquote_prefix = |depth: usize| -> String { "> ".repeat(depth) };

  for event in parser {
    match event {
      Event::Start(Tag::Heading { .. }) => {
        in_heading = true;
        heading_text.clear();
      }
      Event::End(TagEnd::Heading(_)) => {
        in_heading = false;
        if !output.is_empty() && !output.ends_with('\n') {
          output.push('\n');
        }
        output.push_str(&heading_text.to_uppercase());
        output.push('\n');
        at_line_start = true;
      }
      Event::Start(Tag::Paragraph) => {}
      Event::End(TagEnd::Paragraph) => {
        if table_state.is_none() {
          if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
          }
          at_line_start = true;
        }
      }
      Event::Start(Tag::BlockQuote(_)) => {
        blockquote_depth += 1;
      }
      Event::End(TagEnd::BlockQuote(_)) => {
        blockquote_depth = blockquote_depth.saturating_sub(1);
      }
      Event::Start(Tag::List(start)) => {
        let kind = match start {
          Some(n) => ListKind::Ordered(n),
          None => ListKind::Unordered,
        };
        list_stack.push(kind);
      }
      Event::End(TagEnd::List(_)) => {
        list_stack.pop();
      }
      Event::Start(Tag::Item) => {
        if !output.is_empty() && !output.ends_with('\n') {
          output.push('\n');
        }
        if blockquote_depth > 0 {
          output.push_str(&blockquote_prefix(blockquote_depth));
        }
        match list_stack.last_mut() {
          Some(ListKind::Ordered(n)) => {
            output.push_str(&format!("{n}. "));
            *n += 1;
          }
          Some(ListKind::Unordered) => {
            output.push_str("- ");
          }
          None => {}
        }
        at_line_start = false;
      }
      Event::End(TagEnd::Item) => {
        if !output.ends_with('\n') {
          output.push('\n');
        }
        at_line_start = true;
      }
      Event::Start(Tag::CodeBlock(_)) => {
        if !output.is_empty() && !output.ends_with('\n') {
          output.push('\n');
        }
        in_code_block = true;
      }
      Event::End(TagEnd::CodeBlock) => {
        in_code_block = false;
        at_line_start = true;
      }
      Event::Start(Tag::Link { dest_url, .. }) => {
        in_link = true;
        link_url = dest_url.to_string();
        link_text.clear();
      }
      Event::End(TagEnd::Link) => {
        in_link = false;
        if in_heading {
          heading_text.push_str(&link_text);
          if link_url != link_text {
            heading_text.push_str(&format!(" ({link_url})"));
          }
        } else if let Some(ref mut ts) = table_state {
          ts.current_cell.push_str(&link_text);
          if link_url != link_text {
            ts.current_cell.push_str(&format!(" ({link_url})"));
          }
        } else {
          output.push_str(&link_text);
          if link_url != link_text {
            output.push_str(&format!(" ({link_url})"));
          }
        }
      }
      Event::Start(Tag::Image { dest_url, .. }) => {
        in_image = true;
        image_url = dest_url.to_string();
        image_text.clear();
      }
      Event::End(TagEnd::Image) => {
        in_image = false;
        let rendered = if image_text.is_empty() {
          image_url.clone()
        } else {
          format!("{image_text} ({image_url})")
        };
        if in_heading {
          heading_text.push_str(&rendered);
        } else if let Some(ref mut ts) = table_state {
          ts.current_cell.push_str(&rendered);
        } else {
          output.push_str(&rendered);
        }
      }
      Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough) => {}
      Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {}
      Event::Start(Tag::Superscript) => {
        let target = if in_heading {
          &mut heading_text
        } else if let Some(ref mut ts) = table_state {
          &mut ts.current_cell
        } else {
          &mut output
        };
        target.push('^');
      }
      Event::End(TagEnd::Superscript) => {}
      Event::Start(Tag::Subscript) => {
        let target = if in_heading {
          &mut heading_text
        } else if let Some(ref mut ts) = table_state {
          &mut ts.current_cell
        } else {
          &mut output
        };
        target.push('_');
      }
      Event::End(TagEnd::Subscript) => {}
      Event::Start(Tag::Table(_)) => {
        if !output.is_empty() && !output.ends_with('\n') {
          output.push('\n');
        }
        table_state = Some(TableState {
          rows: Vec::new(),
          current_row: Vec::new(),
          current_cell: String::new(),
        });
      }
      Event::End(TagEnd::Table) => {
        if let Some(ts) = table_state.take() {
          for row in &ts.rows {
            let line = row
              .iter()
              .map(|cell| cell.trim())
              .collect::<Vec<_>>()
              .join(" | ");
            output.push_str(&line);
            output.push('\n');
          }
        }
        at_line_start = true;
      }
      Event::Start(Tag::TableHead | Tag::TableRow) => {
        if let Some(ref mut ts) = table_state {
          ts.current_row = Vec::new();
        }
      }
      Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
        if let Some(ref mut ts) = table_state {
          let row = std::mem::take(&mut ts.current_row);
          ts.rows.push(row);
        }
      }
      Event::Start(Tag::TableCell) => {
        if let Some(ref mut ts) = table_state {
          ts.current_cell = String::new();
        }
      }
      Event::End(TagEnd::TableCell) => {
        if let Some(ref mut ts) = table_state {
          let cell = std::mem::take(&mut ts.current_cell);
          ts.current_row.push(cell);
        }
      }
      Event::Text(text) => {
        if in_link {
          link_text.push_str(&text);
        } else if in_image {
          image_text.push_str(&text);
        } else if in_heading {
          heading_text.push_str(&text);
        } else if in_code_block {
          if blockquote_depth > 0 {
            for (i, line) in text.lines().enumerate() {
              if i > 0 || at_line_start {
                output.push_str(&blockquote_prefix(blockquote_depth));
              }
              output.push_str(line);
              output.push('\n');
            }
          } else {
            output.push_str(&text);
          }
          at_line_start = text.ends_with('\n');
        } else if let Some(ref mut ts) = table_state {
          ts.current_cell.push_str(&text);
        } else {
          if at_line_start && blockquote_depth > 0 && list_stack.is_empty() {
            output.push_str(&blockquote_prefix(blockquote_depth));
          }
          output.push_str(&text);
          at_line_start = false;
        }
      }
      Event::Code(text) => {
        if in_link {
          link_text.push_str(&text);
        } else if in_image {
          image_text.push_str(&text);
        } else if in_heading {
          heading_text.push_str(&text);
        } else if let Some(ref mut ts) = table_state {
          ts.current_cell.push_str(&text);
        } else {
          output.push_str(&text);
          at_line_start = false;
        }
      }
      Event::SoftBreak | Event::HardBreak => {
        if in_heading {
          heading_text.push(' ');
        } else if let Some(ref mut ts) = table_state {
          ts.current_cell.push(' ');
        } else {
          output.push('\n');
          at_line_start = true;
        }
      }
      Event::Rule => {
        if !output.is_empty() && !output.ends_with('\n') {
          output.push('\n');
        }
        if blockquote_depth > 0 {
          output.push_str(&blockquote_prefix(blockquote_depth));
        }
        output.push_str("---\n");
        at_line_start = true;
      }
      _ => {}
    }
  }

  while output.ends_with('\n') {
    output.pop();
  }

  output
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

    let at_boundary = end >= s.len() || s.as_bytes()[end] == b' ';

    if !at_boundary {
      if let Some(space) = s[start..end].rfind(' ') {
        end = start + space;
      }
    }

    chunks.push(&s[start..end]);

    start = end;
    if start < s.len() && s.as_bytes()[start] == b' ' {
      start += 1;
    }
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

  #[test]
  fn split_utf8_word_boundary() {
    let chunks = split_utf8("foo bar baz", 7);
    assert_eq!(chunks, vec!["foo bar", "baz"]);
  }

  #[test]
  fn split_utf8_no_space_fallback() {
    let s = "a".repeat(10);
    let chunks = split_utf8(&s, 4);
    assert_eq!(chunks, vec!["aaaa", "aaaa", "aa"]);
  }

  #[test]
  fn markdown_inline_formatting() {
    assert_eq!(markdown_to_plaintext("**foo**"), "foo");
    assert_eq!(markdown_to_plaintext("*foo*"), "foo");
    assert_eq!(markdown_to_plaintext("`foo`"), "foo");
    assert_eq!(markdown_to_plaintext("~~foo~~"), "foo");
  }

  #[test]
  fn markdown_headings() {
    assert_eq!(markdown_to_plaintext("# foo"), "FOO");
    assert_eq!(markdown_to_plaintext("## foo bar"), "FOO BAR");
  }

  #[test]
  fn markdown_unordered_list() {
    assert_eq!(
      markdown_to_plaintext("- foo\n- bar\n- baz"),
      "- foo\n- bar\n- baz"
    );
  }

  #[test]
  fn markdown_ordered_list() {
    assert_eq!(
      markdown_to_plaintext("1. foo\n2. bar\n3. baz"),
      "1. foo\n2. bar\n3. baz"
    );
  }

  #[test]
  fn markdown_blockquote() {
    assert_eq!(markdown_to_plaintext("> foo"), "> foo");
  }

  #[test]
  fn markdown_nested_blockquote() {
    assert_eq!(markdown_to_plaintext("> > foo"), "> > foo");
  }

  #[test]
  fn markdown_link() {
    assert_eq!(
      markdown_to_plaintext("[foo](http://bar)"),
      "foo (http://bar)"
    );
  }

  #[test]
  fn markdown_link_same_text_and_url() {
    assert_eq!(markdown_to_plaintext("<http://foo>"), "http://foo");
  }

  #[test]
  fn markdown_image() {
    assert_eq!(
      markdown_to_plaintext("![foo](http://bar)"),
      "foo (http://bar)"
    );
  }

  #[test]
  fn markdown_code_block() {
    assert_eq!(markdown_to_plaintext("```\nfoo\nbar\n```"), "foo\nbar");
  }

  #[test]
  fn markdown_table() {
    assert_eq!(
      markdown_to_plaintext("| a | bb |\n|---|----|\n| c | d  |"),
      "a | bb\nc | d"
    );
  }

  #[test]
  fn markdown_horizontal_rule() {
    assert_eq!(markdown_to_plaintext("foo\n\n---\n\nbar"), "foo\n---\nbar");
  }

  #[test]
  fn markdown_mixed_content() {
    assert_eq!(
      markdown_to_plaintext("# foo\n\nbar **baz**"),
      "FOO\nbar baz"
    );
  }

  #[test]
  fn markdown_superscript() {
    assert_eq!(markdown_to_plaintext("foo ^bar^"), "foo ^bar");
  }

  #[test]
  fn markdown_subscript() {
    assert_eq!(markdown_to_plaintext("foo ~bar~"), "foo _bar");
  }

  #[test]
  fn markdown_tilde_preserved() {
    assert_eq!(
      markdown_to_plaintext("restarted ~1 day ago"),
      "restarted ~1 day ago"
    );
  }
}
