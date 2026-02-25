use super::*;

use std::os::{fd::FromRawFd, unix::net::UnixDatagram};

const SESSION_DIR: &str = "/root/sessions";
const REPO_URL: &str = "git@localhost:root/notebook.git";
const NOTES: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("notes");
const DB_KEY: &str = "notebook";

const SYSTEM_PROMPT: &str = "You are monitoring the notebook repository, \
checked out in the current directory. Each commit should be interpreted as an \
instruction from the user. Please act on each commit using your best judgement, \
in a way that addresses the intent of the user. This is not an interactive \
session. Be proactive.";

#[derive(clap::Args)]
pub(crate) struct Notebook {
  #[arg(long)]
  db: Option<PathBuf>,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
  #[arg(long)]
  reset: bool,
}

impl Notebook {
  pub(crate) fn run(self) -> Result {
    if self.reset {
      let db_path = self.db.clone().unwrap_or_else(db_path);
      let db = redb::Database::create(&db_path).context(error::DatabaseOpen { path: db_path })?;
      let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
      {
        let mut table = write_txn.open_table(NOTES).context(error::DatabaseTable)?;
        table.remove(DB_KEY).context(error::DatabaseStorage)?;
      }
      write_txn.commit().context(error::DatabaseCommit)?;
      return Ok(());
    }

    let socket = unsafe { UnixDatagram::from_raw_fd(3) };

    let mut buf = [0u8; 4096];

    loop {
      let n = socket.recv(&mut buf).context(error::SocketRecv)?;
      let payload = String::from_utf8_lossy(&buf[..n]);
      let parts: Vec<&str> = payload.split_whitespace().collect();

      if parts.len() < 2 {
        continue;
      }

      let (oldrev, newrev) = (parts[0], parts[1]);

      if let Err(e) = self.handle_message(oldrev, newrev) {
        ::log::error!("failed to handle notebook message: {e}");
      }
    }
  }

  fn handle_message(&self, oldrev: &str, newrev: &str) -> Result {
    let (session, resume) = self.lookup_session()?;

    let session_dir = Path::new(SESSION_DIR).join(&session);

    Self::clone_or_pull(&session_dir)?;

    let (subject, prompt) = Self::build_prompt(&session_dir, oldrev, newrev)?;

    let response = invoke_agent(
      &self.claude,
      Path::new(SESSION_DIR),
      &session,
      resume,
      &prompt,
      Some(SYSTEM_PROMPT),
      false,
    )?;

    if !resume {
      self.save_session(&session)?;
    }

    let response = response.trim();

    notify::send(&format!("note complete: {subject} {response}"))
  }

  fn clone_or_pull(session_dir: &Path) -> Result {
    if session_dir.join(".git").exists() {
      Command::new("git")
        .arg("-C")
        .arg(session_dir)
        .arg("pull")
        .output()
        .context(error::GitSync)?;
    } else {
      Command::new("git")
        .arg("clone")
        .arg(REPO_URL)
        .arg(session_dir)
        .output()
        .context(error::GitSync)?;
    }

    Ok(())
  }

  fn build_prompt(session_dir: &Path, oldrev: &str, newrev: &str) -> Result<(String, String)> {
    let output = Command::new("git")
      .arg("-C")
      .arg(session_dir)
      .args(["log", "-1", "--format=%s", newrev])
      .output()
      .context(error::GitInfo)?;

    let subject = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let status = Command::new("git")
      .arg("-C")
      .arg(session_dir)
      .args(["merge-base", "--is-ancestor", oldrev, newrev])
      .status()
      .context(error::GitInfo)?;

    let push_type = if status.success() {
      "fast-forward"
    } else {
      "force push"
    };

    let short_new = &newrev[..newrev.len().min(7)];
    let short_old = &oldrev[..oldrev.len().min(7)];

    Ok((
      subject.clone(),
      format!("Commit {short_new}: {subject}\nPrevious: {short_old}\nType: {push_type}"),
    ))
  }

  fn lookup_session(&self) -> Result<(String, bool)> {
    let db_path = self.db.clone().unwrap_or_else(db_path);
    let db = redb::Database::create(&db_path).context(error::DatabaseOpen { path: db_path })?;

    let read_txn = db.begin_read().context(error::DatabaseTransaction)?;
    let table = read_txn.open_table(NOTES);

    let existing = match table {
      Ok(table) => table
        .get(DB_KEY)
        .context(error::DatabaseStorage)?
        .map(|v| v.value().to_string()),
      Err(redb::TableError::TableDoesNotExist(_)) => None,
      Err(e) => return Err(e).context(error::DatabaseTable),
    };

    let resume = existing.is_some();
    let session = existing.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    Ok((session, resume))
  }

  fn save_session(&self, session: &str) -> Result {
    let db_path = self.db.clone().unwrap_or_else(db_path);
    let db = redb::Database::create(&db_path).context(error::DatabaseOpen { path: db_path })?;

    let write_txn = db.begin_write().context(error::DatabaseTransaction)?;
    {
      let mut table = write_txn.open_table(NOTES).context(error::DatabaseTable)?;
      table
        .insert(DB_KEY, session)
        .context(error::DatabaseStorage)?;
    }
    write_txn.commit().context(error::DatabaseCommit)?;

    Ok(())
  }
}
