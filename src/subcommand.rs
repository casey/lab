mod chat;
mod mail;

use super::*;

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
