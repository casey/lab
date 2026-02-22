use super::*;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)), visibility(pub(crate)))]
pub(crate) enum Error {
  #[snafu(display("I/O error at `{}`", path.display()))]
  FilesystemIo { path: PathBuf, source: io::Error },
  #[snafu(display("failed to parse message"))]
  MailParse { source: mailparse::MailParseError },
  #[snafu(display("message has no Message-ID header"))]
  MissingMessageId,
  #[snafu(display("message has no sender"))]
  MissingSender,
  #[snafu(display("invalid recipient address"))]
  Address {
    source: lettre::address::AddressError,
  },
  #[snafu(display("failed to send reply"))]
  Send {
    source: lettre::transport::sendmail::Error,
  },
  #[snafu(display("failed to read stdin"))]
  Stdin { source: io::Error },
}

impl Error {
  pub(crate) fn exit_code(&self) -> ExitCode {
    match self {
      Self::FilesystemIo { .. } => ExitCode::from(75),
      _ => ExitCode::FAILURE,
    }
  }
}
