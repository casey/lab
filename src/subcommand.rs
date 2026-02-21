mod mail;

use super::*;

#[derive(clap::Subcommand)]
pub(crate) enum Subcommand {
  Mail(mail::Mail),
}

impl Subcommand {
  pub(crate) fn run(self) -> Result {
    match self {
      Self::Mail(mail) => mail.run(),
    }
  }
}
