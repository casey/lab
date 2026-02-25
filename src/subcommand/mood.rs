use super::*;

#[derive(clap::Args)]
pub(crate) struct Mood {
  emoji: String,
}

impl Mood {
  pub(crate) fn run(self) -> Result {
    #[cfg(target_os = "linux")]
    {
      let logger = systemd_journal_logger::JournalLog::empty()
        .context(error::JournalSend)?
        .with_syslog_identifier("agent".into());

      logger
        .journal_send(
          &::log::Record::builder()
            .level(::log::Level::Info)
            .args(format_args!("Mood: {}", self.emoji))
            .build(),
        )
        .context(error::JournalSend)?;
    }

    #[cfg(not(target_os = "linux"))]
    {
      ::log::info!("Mood: {}", self.emoji);
    }

    Ok(())
  }
}
