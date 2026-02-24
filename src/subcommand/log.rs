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

fn strip_prefix<'a>(path: &'a str, cwd: &str) -> &'a str {
  path
    .strip_prefix(cwd)
    .and_then(|p| p.strip_prefix('/'))
    .unwrap_or(path)
}

fn tool_detail(
  tool_name: &str,
  tool_input: &serde_json::Value,
  cwd: Option<&str>,
) -> Option<String> {
  match tool_name {
    "Read" | "Write" | "Edit" => {
      let path = tool_input.get("file_path").and_then(|v| v.as_str())?;
      Some(match cwd {
        Some(cwd) => strip_prefix(path, cwd).to_owned(),
        None => path.to_owned(),
      })
    }
    "Bash" => tool_input
      .get("command")
      .and_then(|v| v.as_str())
      .map(|s| s.to_owned()),
    "Grep" => {
      let pattern = tool_input.get("pattern").and_then(|v| v.as_str())?;
      let glob = tool_input.get("glob").and_then(|v| v.as_str());
      Some(match glob {
        Some(g) => format!("'{pattern}' {g}"),
        None => format!("'{pattern}'"),
      })
    }
    "Glob" => tool_input
      .get("pattern")
      .and_then(|v| v.as_str())
      .map(|s| s.to_owned()),
    "Task" => tool_input
      .get("description")
      .and_then(|v| v.as_str())
      .map(|s| s.to_owned()),
    _ => None,
  }
}

fn build_message(object: &serde_json::Map<String, serde_json::Value>) -> String {
  let hook_event_name = object.get("hook_event_name").and_then(|v| v.as_str());
  let tool_name = object.get("tool_name").and_then(|v| v.as_str());
  let tool_input = object.get("tool_input");
  let cwd = object.get("cwd").and_then(|v| v.as_str());

  match (hook_event_name, tool_name) {
    (Some(event @ ("PreToolUse" | "PostToolUse")), Some(tool)) => {
      match tool_input.and_then(|input| tool_detail(tool, input, cwd)) {
        Some(detail) => format!("{event} {tool}: {detail}"),
        None => format!("{event} {tool}"),
      }
    }
    (Some("UserPromptSubmit"), _) => match object.get("prompt").and_then(|v| v.as_str()) {
      Some(prompt) => format!("Prompt: {prompt}"),
      None => "Prompt".to_owned(),
    },
    (Some(event), Some(tool)) => format!("{event} {tool}"),
    (Some(event), None) => event.to_owned(),
    _ => serde_json::to_string(object).unwrap_or_default(),
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

    let message = match object {
      Some(obj) => build_message(obj),
      None => input.trim().to_owned(),
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

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(json: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(json)
      .unwrap()
      .as_object()
      .unwrap()
      .clone()
  }

  #[test]
  fn strip_prefix_removes_cwd() {
    assert_eq!(
      strip_prefix("/root/src/lab/src/main.rs", "/root/src/lab"),
      "src/main.rs"
    );
  }

  #[test]
  fn strip_prefix_no_match() {
    assert_eq!(
      strip_prefix("/other/path/foo.rs", "/root/src/lab"),
      "/other/path/foo.rs"
    );
  }

  #[test]
  fn message_read() {
    #[track_caller]
    fn case(event: &str) {
      let obj = parse(&format!(
        r#"{{"hook_event_name":"{event}","tool_name":"Read","tool_input":{{"file_path":"/root/src/lab/src/main.rs"}},"cwd":"/root/src/lab"}}"#
      ));
      assert_eq!(build_message(&obj), format!("{event} Read: src/main.rs"));
    }

    case("PreToolUse");
    case("PostToolUse");
  }

  #[test]
  fn message_write() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Write","tool_input":{"file_path":"/root/foo.rs"},"cwd":"/root"}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Write: foo.rs");
  }

  #[test]
  fn message_edit() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"/root/foo.rs"},"cwd":"/root"}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Edit: foo.rs");
  }

  #[test]
  fn message_bash() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test --workspace"}}"#,
    );
    assert_eq!(
      build_message(&obj),
      "PreToolUse Bash: cargo test --workspace"
    );
  }

  #[test]
  fn message_grep_with_glob() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Grep","tool_input":{"pattern":"foo","glob":"**/*.rs"}}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Grep: 'foo' **/*.rs");
  }

  #[test]
  fn message_grep_without_glob() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Grep","tool_input":{"pattern":"foo"}}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Grep: 'foo'");
  }

  #[test]
  fn message_glob() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Glob","tool_input":{"pattern":"**/*.rs"}}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Glob: **/*.rs");
  }

  #[test]
  fn message_task() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Task","tool_input":{"description":"explore codebase"}}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Task: explore codebase");
  }

  #[test]
  fn message_unknown_tool() {
    let obj = parse(r#"{"hook_event_name":"PreToolUse","tool_name":"FooTool","tool_input":{}}"#);
    assert_eq!(build_message(&obj), "PreToolUse FooTool");
  }

  #[test]
  fn message_user_prompt() {
    let obj = parse(r#"{"hook_event_name":"UserPromptSubmit","prompt":"hello world"}"#);
    assert_eq!(build_message(&obj), "Prompt: hello world");
  }

  #[test]
  fn message_user_prompt_no_text() {
    let obj = parse(r#"{"hook_event_name":"UserPromptSubmit"}"#);
    assert_eq!(build_message(&obj), "Prompt");
  }

  #[test]
  fn message_session_start() {
    let obj = parse(r#"{"hook_event_name":"SessionStart"}"#);
    assert_eq!(build_message(&obj), "SessionStart");
  }

  #[test]
  fn message_stop() {
    let obj = parse(r#"{"hook_event_name":"Stop"}"#);
    assert_eq!(build_message(&obj), "Stop");
  }

  #[test]
  fn message_no_cwd() {
    let obj = parse(
      r#"{"hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{"file_path":"/root/foo.rs"}}"#,
    );
    assert_eq!(build_message(&obj), "PreToolUse Read: /root/foo.rs");
  }
}
