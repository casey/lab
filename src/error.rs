use super::*;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)), visibility(pub(crate)))]
pub(crate) enum Error {
  #[snafu(display("I/O error at `{}`", path.display()))]
  Io { path: PathBuf, source: io::Error },
  #[snafu(display("failed to save to maildir at `{}`", path.display()))]
  MaildirSave { path: PathBuf, source: io::Error },
  #[snafu(display("failed to parse message"))]
  MailParse { source: mailparse::MailParseError },
  #[snafu(display("failed to read stdin"))]
  Stdin { source: io::Error },
  #[snafu(display("sendmail exited with {status}"))]
  Sendmail { status: process::ExitStatus },
}

impl Error {
  pub(crate) fn exit_code(&self) -> ExitCode {
    match self {
      Self::MaildirSave { .. } => ExitCode::from(75),
      _ => ExitCode::FAILURE,
    }
  }
}
