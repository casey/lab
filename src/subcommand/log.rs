use super::*;

#[derive(clap::Args)]
pub(crate) struct Log {}

struct Fields(Vec<(String, String)>);

impl ::log::kv::Source for Fields {
  fn visit<'kvs>(
    &'kvs self,
    visitor: &mut dyn ::log::kv::VisitSource<'kvs>,
  ) -> std::result::Result<(), ::log::kv::Error> {
    for (key, value) in &self.0 {
      visitor.visit_pair(
        ::log::kv::Key::from(key.as_str()),
        ::log::kv::Value::from(value.as_str()),
      )?;
    }
    Ok(())
  }
}

impl Log {
  pub(crate) fn run(self) -> Result {
    let mut input = String::new();
    io::stdin()
      .read_to_string(&mut input)
      .context(error::Stdin)?;

    let json = serde_json::from_str::<serde_json::Value>(&input).context(error::JsonParse)?;

    let object = json.as_object();

    let fields = object
      .map(|obj| {
        obj
          .iter()
          .map(|(key, value)| {
            let value = if let Some(s) = value.as_str() {
              s.to_owned()
            } else {
              value.to_string()
            };
            (key.clone(), value)
          })
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();

    let hook_event_name = object
      .and_then(|obj| obj.get("hook_event_name"))
      .and_then(|v| v.as_str());

    let tool_name = object
      .and_then(|obj| obj.get("tool_name"))
      .and_then(|v| v.as_str());

    let message = match (hook_event_name, tool_name) {
      (Some(event), Some(tool)) => format!("{event} {tool}"),
      (Some(event), None) => event.to_owned(),
      _ => input.trim().to_owned(),
    };

    let kvs = Fields(fields);

    #[cfg(target_os = "linux")]
    {
      let logger = systemd_journal_logger::JournalLog::empty()
        .context(error::JournalSend)?
        .with_syslog_identifier("agent".into());

      logger
        .journal_send(
          &::log::Record::builder()
            .level(::log::Level::Info)
            .key_values(&kvs)
            .args(format_args!("{}", message))
            .build(),
        )
        .context(error::JournalSend)?;
    }

    #[cfg(not(target_os = "linux"))]
    {
      let _ = kvs;
      ::log::info!("{message}");
    }

    Ok(())
  }
}
