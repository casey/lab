use super::*;

use std::os::{fd::FromRawFd, unix::net::UnixDatagram};

const REPO_URL: &str = "git@localhost:root/notebook.git";
const SESSION_NAME: &str = "notebook";

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
      let db = self.db.clone().unwrap_or_else(db_path);
      return reset_session(&db, SESSION_NAME);
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
    let db = self.db.clone().unwrap_or_else(db_path);
    let (session, resume) = lookup_session(&db, SESSION_NAME)?;

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
      save_session(&db, SESSION_NAME, &session)?;
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
}
