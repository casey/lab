use super::*;

#[derive(clap::Args)]
pub(crate) struct Note {
  message: String,
}

impl Note {
  pub(crate) fn run(self) -> Result {
    notify::send(&format!("note: {}", self.message))
  }
}
