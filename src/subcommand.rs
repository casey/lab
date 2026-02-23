mod chat;
mod mail;

use super::*;

pub(crate) fn db_path() -> PathBuf {
  dirs::home_dir().unwrap().join(".lab.redb")
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

  String::from_utf8(output.stdout).context(error::AgentOutput)
}

#[derive(clap::Subcommand)]
pub(crate) enum Subcommand {
  Chat(chat::Chat),
  Mail(mail::Mail),
}

impl Subcommand {
  pub(crate) fn run(self) -> Result {
    match self {
      Self::Chat(chat) => chat.run(),
      Self::Mail(mail) => mail.run(),
    }
  }
}
