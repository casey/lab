use super::*;

use std::os::unix::process::CommandExt as _;

#[derive(Clone)]
enum Session {
  Uuid(uuid::Uuid),
  Name(String),
}

impl std::str::FromStr for Session {
  type Err = std::convert::Infallible;

  fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
    Ok(match s.parse::<uuid::Uuid>() {
      Ok(uuid) => Self::Uuid(uuid),
      Err(_) => Self::Name(s.to_string()),
    })
  }
}

#[derive(clap::Args)]
pub(crate) struct Resume {
  session: Session,
  #[arg(long)]
  db: Option<PathBuf>,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
}

impl Resume {
  pub(crate) fn run(self) -> Result {
    let uuid = match self.session {
      Session::Uuid(uuid) => uuid.to_string(),
      Session::Name(name) => {
        let db_path = self.db.unwrap_or_else(db_path);
        let (uuid, resume) = lookup_session(&db_path, &name)?;
        if !resume {
          return Err(Error::SessionNotFound { name });
        }
        uuid
      }
    };

    let session_dir = Path::new(SESSION_DIR).join(&uuid);

    let err = Command::new(&self.claude)
      .arg("--resume")
      .arg(&uuid)
      .current_dir(&session_dir)
      .exec();

    Err(Error::AgentInvocation { source: err })
  }
}
