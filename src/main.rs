use {
  crate::{error::Error, subcommand::Subcommand},
  clap::Parser,
  mailparse::MailHeaderMap,
  snafu::{ResultExt, Snafu},
  std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{self, Command, ExitCode, Stdio},
    time::{SystemTime, UNIX_EPOCH},
  },
};

mod error;
mod subcommand;

type Result<T = (), E = Error> = std::result::Result<T, E>;

#[derive(Parser)]
struct Arguments {
  #[command(subcommand)]
  subcommand: Subcommand,
}

fn main() -> ExitCode {
  if let Err(err) = systemd_journal_logger::JournalLog::new().map(|l| l.install()) {
    eprintln!("error: failed to initialize logger: {err}");
    return ExitCode::FAILURE;
  }

  log::set_max_level(log::LevelFilter::Info);

  if let Err(err) = Arguments::parse().subcommand.run() {
    eprintln!("error: {err}");
    for (i, source) in snafu::CleanedErrorText::new(&err).skip(1).enumerate() {
      eprintln!("  {}: {}", i, source.1);
    }
    err.exit_code()
  } else {
    ExitCode::SUCCESS
  }
}
