mod chat;
mod log;
mod mail;
mod mood;
mod note;
mod notebook;
mod notify;
mod reset;
mod resume;
mod sessions;
mod task;

use super::*;

const SESSION_DIR: &str = "/root/sessions";
const SESSIONS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("sessions");

pub(crate) fn db_path() -> PathBuf {
  dirs::home_dir().unwrap().join(".lab.redb")
}

pub(crate) fn lookup_session(db_path: &Path, name: &str) -> Result<(String, bool)> {
  let db = redb::Database::create(db_path).context(error::DatabaseOpen { path: db_path })?;

  let read_txn = db.begin_read().context(error::DatabaseTransaction)?;
  let table = read_txn.open_table(SESSIONS);

  let existing = match table {
    Ok(table) => table
      .get(name)
      .context(error::DatabaseStorage)?
      .map(|v| v.value().to_string()),
    Err(redb::TableError::TableDoesNotExist(_)) => None,
    Err(e) => return Err(e).context(error::DatabaseTable),
  };

  let resume = existing.is_some();
  let session = existing.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

  Ok((session, resume))
}

pub(crate) fn save_session(db_path: &Path, name: &str, session: &str) -> Result {
  let db = redb::Database::create(db_path).context(error::DatabaseOpen { path: db_path })?;

  let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
  {
    let mut table = write_txn
      .open_table(SESSIONS)
      .context(error::DatabaseTable)?;
    table
      .insert(name, session)
      .context(error::DatabaseStorage)?;
  }
  write_txn.commit().context(error::DatabaseCommit)?;

  Ok(())
}

pub(crate) fn reset_session(db_path: &Path, name: &str) -> Result {
  let db = redb::Database::create(db_path).context(error::DatabaseOpen { path: db_path })?;

  let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
  {
    let mut table = write_txn
      .open_table(SESSIONS)
      .context(error::DatabaseTable)?;
    table.remove(name).context(error::DatabaseStorage)?;
  }
  write_txn.commit().context(error::DatabaseCommit)?;

  Ok(())
}

pub(crate) fn invoke_agent(
  claude: &Path,
  session_dir: &Path,
  session: &str,
  resume: bool,
  body: &str,
  system_prompt: Option<&str>,
  fast: bool,
) -> Result<String> {
  let session_dir = session_dir.join(session);

  fs::create_dir_all(&session_dir).context(error::SessionDir {
    path: session_dir.clone(),
  })?;

  let mut command = Command::new(claude);
  command
    .arg("--print")
    .arg("--dangerously-skip-permissions")
    .env("IS_SANDBOX", "1");

  if fast {
    command.args(["--settings", r#"{"fastMode": true}"#]);
  }

  if resume {
    command.arg("--resume").arg(session);
  } else {
    let prompt = if let Some(extra) = system_prompt {
      format!("Your session ID is {session}. {extra}")
    } else {
      format!("Your session ID is {session}.")
    };
    command
      .arg("--session-id")
      .arg(session)
      .arg("--append-system-prompt")
      .arg(prompt);
  }

  let output = command
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

  let response = String::from_utf8(output.stdout).context(error::AgentOutput)?;

  if response.trim().is_empty() {
    let output = Command::new(claude)
      .arg("--print")
      .arg("--dangerously-skip-permissions")
      .env("IS_SANDBOX", "1")
      .arg("--resume")
      .arg(session)
      .stdin(process::Stdio::piped())
      .stdout(process::Stdio::piped())
      .stderr(process::Stdio::piped())
      .current_dir(&session_dir)
      .spawn()
      .and_then(|mut child| {
        use io::Write;
        if let Some(mut stdin) = child.stdin.take() {
          stdin.write_all(b"Briefly summarize what you just did.")?;
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

    return String::from_utf8(output.stdout).context(error::AgentOutput);
  }

  Ok(response)
}

#[derive(clap::Subcommand)]
pub(crate) enum Subcommand {
  Chat(chat::Chat),
  Log(log::Log),
  Mail(mail::Mail),
  Mood(mood::Mood),
  Note(note::Note),
  Notebook(notebook::Notebook),
  Notify(notify::Notify),
  Reset(reset::Reset),
  Resume(resume::Resume),
  Sessions(sessions::Sessions),
  Task(task::Task),
}

impl Subcommand {
  pub(crate) fn run(self) -> Result {
    match self {
      Self::Chat(chat) => chat.run(),
      Self::Log(log) => log.run(),
      Self::Mail(mail) => mail.run(),
      Self::Mood(mood) => mood.run(),
      Self::Note(note) => note.run(),
      Self::Notebook(notebook) => notebook.run(),
      Self::Notify(notify) => notify.run(),
      Self::Reset(reset) => reset.run(),
      Self::Resume(resume) => resume.run(),
      Self::Sessions(sessions) => sessions.run(),
      Self::Task(task) => task.run(),
    }
  }
}
