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
  #[snafu(display("sendmail exited with {status}"))]
  Sendmail { status: process::ExitStatus },
  #[snafu(display("failed to invoke sendmail"))]
  SendmailInvoke { source: io::Error },
  #[snafu(display("failed to write to sendmail stdin"))]
  SendmailStdin { source: io::Error },
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
