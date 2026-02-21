use snafu::Snafu;
use std::io;
use std::process::ExitCode;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)), visibility(pub(crate)))]
pub(crate) enum Error {
  #[snafu(display("I/O error at `{path}`"))]
  Io { path: String, source: io::Error },
  #[snafu(display("failed to save to maildir at `{path}`"))]
  MaildirSave { path: String, source: io::Error },
  #[snafu(display("failed to parse message"))]
  MailParse { source: mailparse::MailParseError },
  #[snafu(display("failed to read stdin"))]
  Stdin { source: io::Error },
  #[snafu(display("sendmail exited with {status}"))]
  Sendmail { status: std::process::ExitStatus },
}

impl Error {
  pub(crate) fn exit_code(&self) -> ExitCode {
    match self {
      Self::MaildirSave { .. } => ExitCode::from(75),
      _ => ExitCode::FAILURE,
    }
  }
}
