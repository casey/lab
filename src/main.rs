use {
  crate::{error::Error, message::Message, subcommand::Subcommand},
  clap::Parser,
  mailparse::MailHeaderMap,
  redb::ReadableDatabase,
  snafu::{ResultExt, Snafu},
  std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{self, Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
  },
};

mod error;
mod message;
mod subcommand;

type Result<T = (), E = Error> = std::result::Result<T, E>;

#[derive(Parser)]
struct Arguments {
  #[command(subcommand)]
  subcommand: Subcommand,
}

fn main() -> ExitCode {
  if let Ok(logger) = systemd_journal_logger::JournalLog::new() {
    let _ = logger.install();
    log::set_max_level(log::LevelFilter::Info);
  }

  if let Err(err) = Arguments::parse().subcommand.run() {
    eprintln!("error: {err}");
    for (i, source) in snafu::CleanedErrorText::new(&err).skip(1).enumerate() {
      eprintln!("  {}: {}", i, source.1);
    }
    ExitCode::FAILURE
  } else {
    ExitCode::SUCCESS
  }
}
