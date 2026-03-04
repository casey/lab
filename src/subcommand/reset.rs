use super::*;

#[derive(clap::Args)]
pub(crate) struct Reset {
  session: String,
  #[arg(long)]
  db: Option<PathBuf>,
}

impl Reset {
  pub(crate) fn run(self) -> Result {
    let db_path = self.db.unwrap_or_else(db_path);
    reset_session(&db_path, &self.session)
  }
}
