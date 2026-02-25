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
  #[snafu(display("failed to open database at `{}`", path.display()))]
  DatabaseOpen {
    path: PathBuf,
    source: redb::DatabaseError,
  },
  #[snafu(display("database transaction error"))]
  DatabaseTransaction { source: redb::TransactionError },
  #[snafu(display("database commit error"))]
  DatabaseCommit { source: redb::CommitError },
  #[snafu(display("database table error"))]
  DatabaseTable { source: redb::TableError },
  #[snafu(display("database storage error"))]
  DatabaseStorage { source: redb::StorageError },
  #[snafu(display("failed to invoke agent"))]
  AgentInvocation { source: io::Error },
  #[snafu(display("agent exited with {status}\n{stderr}"))]
  AgentFailed {
    status: process::ExitStatus,
    stderr: String,
  },
  #[snafu(display("agent output is not valid UTF-8"))]
  AgentOutput { source: std::string::FromUtf8Error },
  #[snafu(display("failed to create session directory at `{}`", path.display()))]
  SessionDir { path: PathBuf, source: io::Error },
  #[snafu(display("IRC error"))]
  Irc { source: irc::error::Error },
  #[snafu(display("failed to read password file `{}`", path.display()))]
  PasswordFile { path: PathBuf, source: io::Error },
  #[snafu(display("failed to create tokio runtime"))]
  TokioRuntime { source: io::Error },
  #[snafu(display("spawn_blocking task panicked"))]
  TokioJoin { source: tokio::task::JoinError },
  #[snafu(display("IRC protocol error: {message}"))]
  IrcProtocol { message: String },
  #[snafu(display("failed to parse JSON"))]
  JsonParse { source: serde_json::Error },
  #[snafu(display("failed to send to notebook socket"))]
  SocketSend { source: io::Error },
  #[snafu(display("failed to receive from notebook socket"))]
  SocketRecv { source: io::Error },
  #[snafu(display("failed to sync notebook repo"))]
  GitSync { source: io::Error },
  #[snafu(display("failed to read commit info"))]
  GitInfo { source: io::Error },
  #[cfg(target_os = "linux")]
  #[snafu(display("failed to send to journal"))]
  JournalSend { source: io::Error },
}
