use super::*;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)), visibility(pub(crate)))]
pub(crate) enum Error {
  #[snafu(display("failed to spawn claude"))]
  ClaudeSpawn {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("claude exited with {status}: {stderr}"))]
  ClaudeStatus {
    backtrace: Option<Backtrace>,
    status: ExitStatus,
    stderr: String,
  },
  #[snafu(display("claude stderr was not valid UTF-8"))]
  ClaudeStderr {
    backtrace: Option<Backtrace>,
    source: FromUtf8Error,
  },
  #[snafu(display("claude stdout was not valid UTF-8"))]
  ClaudeStdout {
    backtrace: Option<Backtrace>,
    source: FromUtf8Error,
  },
  #[snafu(display("failed to write to claude stdin"))]
  ClaudeStdin {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("failed to wait for claude"))]
  ClaudeWait {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("failed to create thread directory `{}`", path.display()))]
  CreateThreadDir {
    backtrace: Option<Backtrace>,
    path: PathBuf,
    source: io::Error,
  },
  #[snafu(display("failed to commit database transaction"))]
  DatabaseCommit {
    backtrace: Option<Backtrace>,
    source: redb::CommitError,
  },
  #[snafu(display("failed to open database `{}`", path.display()))]
  DatabaseOpen {
    backtrace: Option<Backtrace>,
    path: PathBuf,
    source: redb::DatabaseError,
  },
  #[snafu(display("failed to insert into database table `{table}`"))]
  DatabaseInsert {
    backtrace: Option<Backtrace>,
    table: &'static str,
    source: redb::StorageError,
  },
  #[snafu(display("failed to read from database table `{table}`"))]
  DatabaseRead {
    backtrace: Option<Backtrace>,
    table: &'static str,
    source: redb::StorageError,
  },
  #[snafu(display("failed to open database table `{table}`"))]
  DatabaseTable {
    backtrace: Option<Backtrace>,
    table: &'static str,
    source: redb::TableError,
  },
  #[snafu(display("failed to begin database transaction"))]
  DatabaseTransaction {
    backtrace: Option<Backtrace>,
    source: redb::TransactionError,
  },
  #[snafu(display("failed to parse mail at `{}`", path.display()))]
  MailParse {
    backtrace: Option<Backtrace>,
    path: PathBuf,
    source: mailparse::MailParseError,
  },
  #[snafu(display("failed to read mail directory `{}`", path.display()))]
  ReadMailDir {
    backtrace: Option<Backtrace>,
    path: PathBuf,
    source: io::Error,
  },
  #[snafu(display("failed to read message `{}`", path.display()))]
  ReadMessage {
    backtrace: Option<Backtrace>,
    path: PathBuf,
    source: io::Error,
  },
  #[snafu(display("failed to send reply via sendmail: {stderr}"))]
  Sendmail {
    backtrace: Option<Backtrace>,
    stderr: String,
  },
  #[snafu(display("sendmail stderr was not valid UTF-8"))]
  SendmailStderr {
    backtrace: Option<Backtrace>,
    source: FromUtf8Error,
  },
  #[snafu(display("failed to spawn sendmail"))]
  SendmailSpawn {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("failed to write to sendmail stdin"))]
  SendmailStdin {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("failed to wait for sendmail"))]
  SendmailWait {
    backtrace: Option<Backtrace>,
    source: io::Error,
  },
  #[snafu(display("message references unknown thread (IDs: {ids})"))]
  UnknownThread {
    backtrace: Option<Backtrace>,
    ids: String,
  },
}
