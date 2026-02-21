mod mail;

use crate::Error;

#[derive(clap::Subcommand)]
pub(crate) enum Subcommand {
  Mail(mail::Mail),
}

impl Subcommand {
  pub(crate) fn run(self) -> Result<(), Error> {
    match self {
      Self::Mail(mail) => mail.run(),
    }
  }
}
