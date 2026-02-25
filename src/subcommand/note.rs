use super::*;

use std::os::unix::net::UnixDatagram;

const SOCKET_PATH: &str = "/run/notebook.sock";
const MASTER_REF: &str = "refs/heads/master";

#[derive(clap::Args)]
pub(crate) struct Note {}

impl Note {
  pub(crate) fn run(self) -> Result {
    let stdin = io::read_to_string(io::stdin()).context(error::Stdin)?;

    for line in stdin.lines() {
      let parts: Vec<&str> = line.split_whitespace().collect();

      if parts.len() < 3 || parts[2] != MASTER_REF {
        continue;
      }

      let (oldrev, newrev) = (parts[0], parts[1]);

      let output = Command::new("git")
        .args(["log", "-1", "--format=%s", newrev])
        .output()
        .context(error::GitInfo)?;

      let subject = String::from_utf8_lossy(&output.stdout).trim().to_string();

      notify::send(&format!("note: {subject}"))?;

      let payload = format!("{oldrev} {newrev}");
      let socket = UnixDatagram::unbound().context(error::SocketSend)?;
      socket
        .send_to(payload.as_bytes(), SOCKET_PATH)
        .context(error::SocketSend)?;
    }

    Ok(())
  }
}
