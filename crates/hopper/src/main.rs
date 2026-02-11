use {
  error::Error,
  mailparse::MailHeaderMap,
  redb::{Database, ReadableTable, Table, TableDefinition, TableHandle},
  snafu::{Backtrace, GenerateImplicitData, ResultExt, Snafu},
  std::{
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
  },
  uuid::Uuid,
};

mod error;

type Result<T = (), E = Error> = std::result::Result<T, E>;

const MESSAGE_IDS: TableDefinition<&str, &str> = TableDefinition::new("message_ids");
const THREADS: TableDefinition<&str, &str> = TableDefinition::new("threads");

const MAIL_NEW: &str = "/var/mail/mail/new";
const MAIL_CUR: &str = "/var/mail/mail/cur";
const THREAD_DIR: &str = "/root/hopper";

fn main() {
  let home = env::var("HOME").unwrap_or_else(|_| "/root".into());
  let db_path = PathBuf::from(&home).join(".hopper.redb");
  let db = Database::create(&db_path)
    .context(error::DatabaseOpen {
      path: db_path.clone(),
    })
    .expect("failed to open database");

  let mut entries = fs::read_dir(MAIL_NEW)
    .context(error::ReadMailDir {
      path: PathBuf::from(MAIL_NEW),
    })
    .expect("failed to read mail directory")
    .filter_map(|e| e.ok())
    .collect::<Vec<_>>();

  entries.sort_by_key(|e| e.file_name());

  for entry in entries {
    let path = entry.path();
    if let Err(e) = process_message(&db, &path) {
      let raw = fs::read(&path).unwrap_or_default();
      let subject = mailparse::parse_mail(&raw)
        .ok()
        .and_then(|m| m.headers.get_first_value("Subject"))
        .unwrap_or_default();
      let _ = send_error_reply(&subject, &format!("{e}"));
      move_to_cur(&path);
    }
  }
}

fn process_message(db: &Database, path: &Path) -> Result {
  let raw = fs::read(path).context(error::ReadMessage {
    path: path.to_path_buf(),
  })?;
  let parsed = mailparse::parse_mail(&raw).context(error::MailParse {
    path: path.to_path_buf(),
  })?;

  let subject = parsed
    .headers
    .get_first_value("Subject")
    .unwrap_or_default();
  let message_id = parsed
    .headers
    .get_first_value("Message-ID")
    .unwrap_or_default();
  let in_reply_to = parsed.headers.get_first_value("In-Reply-To");
  let references = parsed.headers.get_first_value("References");
  let body = extract_body(&parsed);

  let reference_ids = collect_reference_ids(&in_reply_to, &references);

  let write_txn = db.begin_write().context(error::DatabaseTransaction)?;

  let (slug, original_subject, is_continuation) = {
    let table = write_txn
      .open_table(MESSAGE_IDS)
      .context(error::DatabaseTable {
        table: MESSAGE_IDS.name(),
      })?;
    resolve_thread(&table, &reference_ids, &subject)?
  };

  let thread_dir = PathBuf::from(THREAD_DIR).join(&slug);
  let is_existing_dir = thread_dir.exists();
  fs::create_dir_all(&thread_dir).context(error::CreateThreadDir {
    path: thread_dir.clone(),
  })?;

  let mut cmd = Command::new("claude");
  cmd.arg("--print");
  if is_existing_dir && is_continuation {
    cmd.arg("--continue");
  }
  cmd.current_dir(&thread_dir);
  cmd.stdin(Stdio::piped());
  cmd.stdout(Stdio::piped());
  cmd.stderr(Stdio::piped());

  let mut child = cmd.spawn().context(error::ClaudeSpawn)?;
  child
    .stdin
    .take()
    .expect("failed to open stdin")
    .write_all(body.as_bytes())
    .context(error::ClaudeStdin)?;
  let output = child.wait_with_output().context(error::ClaudeWait)?;

  if !output.status.success() {
    return Err(Error::ClaudeStatus {
      backtrace: Some(Backtrace::generate()),
      status: output.status,
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  let response = String::from_utf8_lossy(&output.stdout);

  let outgoing_message_id = format!("<{}@lab.rodarmor.com>", Uuid::new_v4());

  let reply_references = if !message_id.is_empty() {
    match &references {
      Some(refs) => format!("{refs} {message_id}"),
      None => message_id.clone(),
    }
  } else {
    String::new()
  };

  send_reply(
    &original_subject,
    &response,
    &message_id,
    &reply_references,
    &outgoing_message_id,
  )?;

  {
    let mut table = write_txn
      .open_table(MESSAGE_IDS)
      .context(error::DatabaseTable {
        table: MESSAGE_IDS.name(),
      })?;
    if !message_id.is_empty() {
      table
        .insert(message_id.as_str(), slug.as_str())
        .context(error::DatabaseInsert {
          table: MESSAGE_IDS.name(),
        })?;
    }
    table
      .insert(outgoing_message_id.as_str(), slug.as_str())
      .context(error::DatabaseInsert {
        table: MESSAGE_IDS.name(),
      })?;
  }

  {
    let mut table = write_txn
      .open_table(THREADS)
      .context(error::DatabaseTable {
        table: THREADS.name(),
      })?;
    if table
      .get(slug.as_str())
      .context(error::DatabaseRead {
        table: THREADS.name(),
      })?
      .is_none()
    {
      table
        .insert(slug.as_str(), original_subject.as_str())
        .context(error::DatabaseInsert {
          table: THREADS.name(),
        })?;
    }
  }

  write_txn.commit().context(error::DatabaseCommit)?;

  move_to_cur(path);

  Ok(())
}

fn extract_body(parsed: &mailparse::ParsedMail) -> String {
  if !parsed.subparts.is_empty() {
    for part in &parsed.subparts {
      if part.ctype.mimetype == "text/plain" {
        if let Ok(body) = part.get_body() {
          return body;
        }
      }
    }
    for part in &parsed.subparts {
      if let Ok(body) = part.get_body() {
        return body;
      }
    }
  }
  parsed.get_body().unwrap_or_default()
}

fn collect_reference_ids(in_reply_to: &Option<String>, references: &Option<String>) -> Vec<String> {
  let mut ids = Vec::new();

  if let Some(refs) = references {
    for id in extract_message_ids(refs) {
      if !ids.contains(&id) {
        ids.push(id);
      }
    }
  }

  if let Some(irt) = in_reply_to {
    for id in extract_message_ids(irt) {
      if !ids.contains(&id) {
        ids.push(id);
      }
    }
  }

  ids
}

fn extract_message_ids(s: &str) -> Vec<String> {
  let mut ids = Vec::new();
  let mut rest = s;
  while let Some(start) = rest.find('<') {
    if let Some(end) = rest[start..].find('>') {
      ids.push(rest[start..start + end + 1].to_string());
      rest = &rest[start + end + 1..];
    } else {
      break;
    }
  }
  ids
}

fn resolve_thread(
  table: &Table<&str, &str>,
  reference_ids: &[String],
  subject: &str,
) -> Result<(String, String, bool)> {
  for id in reference_ids {
    if let Some(guard) = table.get(id.as_str()).context(error::DatabaseRead {
      table: MESSAGE_IDS.name(),
    })? {
      let slug = guard.value().to_string();
      let original_subject = strip_prefixes(subject);
      return Ok((slug, original_subject, true));
    }
  }

  if !reference_ids.is_empty() {
    return Err(Error::UnknownThread {
      backtrace: Some(Backtrace::generate()),
      ids: reference_ids.join(", "),
    });
  }

  let original_subject = strip_prefixes(subject);
  let slug = slugify(&original_subject);
  Ok((slug, original_subject, false))
}

fn strip_prefixes(subject: &str) -> String {
  let mut s = subject.trim();
  loop {
    let lower = s.to_lowercase();
    if lower.starts_with("re:") {
      s = s[3..].trim_start();
    } else if lower.starts_with("fwd:") {
      s = s[4..].trim_start();
    } else if lower.starts_with("fw:") {
      s = s[3..].trim_start();
    } else {
      break;
    }
  }
  s.to_string()
}

fn slugify(s: &str) -> String {
  let slug = s
    .to_lowercase()
    .chars()
    .map(|c| if c.is_alphanumeric() { c } else { '-' })
    .collect::<String>();

  let slug = slug.trim_matches('-').to_string();

  let mut result = String::new();
  let mut prev_dash = false;
  for c in slug.chars() {
    if c == '-' {
      if !prev_dash {
        result.push('-');
      }
      prev_dash = true;
    } else {
      result.push(c);
      prev_dash = false;
    }
  }

  if result.len() > 100 {
    result.truncate(100);
    if let Some(last_dash) = result.rfind('-') {
      result.truncate(last_dash);
    }
  }

  result
}

fn send_reply(
  subject: &str,
  body: &str,
  in_reply_to: &str,
  references: &str,
  message_id: &str,
) -> Result {
  let mut cmd = Command::new("/usr/sbin/sendmail");
  cmd.args(["-t", "-odi"]);
  cmd.stdin(Stdio::piped());
  cmd.stdout(Stdio::piped());
  cmd.stderr(Stdio::piped());

  let mut message = String::new();
  message.push_str("To: rodarmor\n");
  message.push_str(&format!("Subject: Re: {subject}\n"));
  message.push_str(&format!("Message-ID: {message_id}\n"));
  if !in_reply_to.is_empty() {
    message.push_str(&format!("In-Reply-To: {in_reply_to}\n"));
  }
  if !references.is_empty() {
    message.push_str(&format!("References: {references}\n"));
  }
  message.push_str("Content-Type: text/plain; charset=utf-8\n");
  message.push('\n');
  message.push_str(body);

  let mut child = cmd.spawn().context(error::SendmailSpawn)?;
  child
    .stdin
    .take()
    .expect("failed to open stdin")
    .write_all(message.as_bytes())
    .context(error::SendmailStdin)?;
  let output = child.wait_with_output().context(error::SendmailWait)?;

  if !output.status.success() {
    return Err(Error::Sendmail {
      backtrace: Some(Backtrace::generate()),
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  Ok(())
}

fn send_error_reply(subject: &str, error: &str) {
  let body = format!("Error processing your message:\n\n{error}");
  let message_id = format!("<{}@lab.rodarmor.com>", Uuid::new_v4());
  let subject = if subject.is_empty() { "error" } else { subject };
  let _ = send_reply(subject, &body, "", "", &message_id);
}

fn move_to_cur(path: &Path) {
  let filename = path.file_name().expect("no filename");
  let mut dest = PathBuf::from(MAIL_CUR);
  let mut name = filename.to_os_string();
  name.push(":2,S");
  dest.push(name);
  let _ = fs::rename(path, dest);
}
