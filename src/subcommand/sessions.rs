use super::*;

use redb::ReadableTable;

#[derive(clap::Args)]
pub(crate) struct Sessions {
  #[arg(long)]
  db: Option<PathBuf>,
}

impl Sessions {
  pub(crate) fn run(self) -> Result {
    let db_path = self.db.unwrap_or_else(db_path);
    let db = redb::Database::create(&db_path).context(error::DatabaseOpen { path: db_path })?;

    let read_txn = db.begin_read().context(error::DatabaseTransaction)?;
    let table = read_txn.open_table(SESSIONS);

    let mut map = serde_json::Map::new();

    match table {
      Ok(table) => {
        for entry in table.iter().context(error::DatabaseStorage)? {
          let entry = entry.context(error::DatabaseStorage)?;
          map.insert(
            entry.0.value().to_string(),
            serde_json::Value::String(entry.1.value().to_string()),
          );
        }
      }
      Err(redb::TableError::TableDoesNotExist(_)) => {}
      Err(e) => return Err(e).context(error::DatabaseTable),
    }

    println!(
      "{}",
      serde_json::to_string_pretty(&map).context(error::JsonParse)?
    );

    Ok(())
  }
}
