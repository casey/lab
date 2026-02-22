use super::*;

fn find_in_path(name: &str) -> String {
  for dir in std::env::var("PATH").unwrap().split(':') {
    let path = std::path::Path::new(dir).join(name);
    if path.exists() {
      return path.to_str().unwrap().to_string();
    }
  }
  panic!("could not find `{name}` in PATH");
}

fn write_script(dir: &std::path::Path, name: &str, script: &str) -> String {
  use std::os::unix::fs::PermissionsExt;
  let path = dir.join(name);
  std::fs::write(&path, script).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path.to_str().unwrap().to_string()
}

fn write_sendmail(dir: &std::path::Path, script: &str) -> String {
  write_script(dir, "sendmail", script)
}

fn write_claude(dir: &std::path::Path, script: &str) -> String {
  write_script(dir, "claude", script)
}

#[test]
fn missing_sender() {
  let sendmail = find_in_path("true");
  let test = Test::new();
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      "/nonexistent",
    ])
    .stdin(b"From: \r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz")
    .stderr("error: message has no sender\n")
    .failure();
}

#[test]
fn missing_message_id() {
  let sendmail = find_in_path("true");
  let test = Test::new();
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      "/nonexistent",
    ])
    .stdin(b"From: foo@bar.com\r\nContent-Type: text/plain\r\n\r\nbaz")
    .stderr("error: message has no Message-ID header\n")
    .failure();
}

#[test]
fn help() {
  Test::new()
    .args(["--help"])
    .stdout_regex("Usage: lab <COMMAND>.*")
    .success();
}

#[test]
fn mail_help() {
  Test::new()
    .args(["mail", "--help"])
    .stdout_regex("Usage: lab mail.*")
    .success();
}

#[test]
fn dir_required() {
  Test::new()
    .args(["mail"])
    .stderr_regex(".*required.*--dir.*")
    .status(2);
}

#[test]
fn db_required() {
  Test::new()
    .args(["mail", "--dir", "/tmp"])
    .stderr_regex(".*required.*--db.*")
    .status(2);
}

#[test]
fn saves_incoming_and_reply() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions = sessions.to_str().unwrap();
  let input = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
  let test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      &claude,
      "--session-dir",
      sessions,
    ])
    .stdin(input)
    .success();

  let new_dir = test.path().join("new");
  let mut files = std::fs::read_dir(&new_dir)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect::<Vec<_>>();
  files.sort();
  assert_eq!(files.len(), 2);

  let first = std::fs::read(&files[0]).unwrap();
  let second = std::fs::read(&files[1]).unwrap();

  let (incoming, reply) = if first == input {
    (first, second)
  } else {
    (second, first)
  };

  assert_eq!(incoming, input);

  let reply_str = std::str::from_utf8(&reply).unwrap();
  assert!(reply_str.contains("From: \"Root\" <root@tulip.farm>"));
  assert!(reply_str.contains("To: <foo@bar.com>"));
  assert!(reply_str.contains("Subject: Re:"));
  assert!(reply_str.contains("In-Reply-To: <foo@bar>"));
  assert!(reply_str.contains("References: <foo@bar>"));
}

#[test]
fn creates_maildir_subdirs() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions = sessions.to_str().unwrap();
  let test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      &claude,
      "--session-dir",
      sessions,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .success();

  assert!(test.path().join("cur").is_dir());
  assert!(test.path().join("new").is_dir());
  assert!(test.path().join("tmp").is_dir());
}

#[test]
fn sendmail_failure() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\nexit 1\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions = sessions.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      &claude,
      "--session-dir",
      sessions,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: failed to send reply\n.*")
    .failure();
}

#[test]
fn sendmail_not_found() {
  let test = Test::new();
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions = sessions.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      "/nonexistent",
      "--db",
      db,
      "--claude",
      &claude,
      "--session-dir",
      sessions,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: failed to send reply\n.*")
    .failure();
}

#[test]
fn unwritable_dir() {
  let sendmail = find_in_path("true");
  let test = Test::new();
  let db = test.path().join("db.redb");
  let db = db.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      "/proc/foo",
      "--sendmail",
      &sendmail,
      "--db",
      db,
      "--claude",
      "/nonexistent",
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: I/O error at `/proc/foo/cur`\n.*")
    .failure();
}

#[test]
fn new_thread_creates_session() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db_str = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions_str = sessions.to_str().unwrap();
  let _test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz")
    .success();

  assert!(db.exists());

  let session_dirs = std::fs::read_dir(&sessions)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect::<Vec<_>>();
  assert_eq!(session_dirs.len(), 1);
}

#[test]
fn existing_thread_reuses_session() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db_str = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions_str = sessions.to_str().unwrap();

  let test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz")
    .success();

  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\necho bar\n");
  let dir = test.path().to_str().unwrap().to_string();
  let _test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(
      b"From: foo@bar.com\r\nMessage-ID: <baz@bar>\r\nReferences: <foo@bar>\r\nIn-Reply-To: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nqux",
    )
    .success();

  let session_dirs = std::fs::read_dir(&sessions)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect::<Vec<_>>();
  assert_eq!(session_dirs.len(), 1);
}

#[test]
fn markdown_conversion() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(
    test.path(),
    "#!/bin/sh\ncat > /dev/null\nprintf '# foo\\n'\n",
  );
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db_str = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions_str = sessions.to_str().unwrap();
  let input = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
  let test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(input)
    .success();

  let new_dir = test.path().join("new");
  let files = std::fs::read_dir(&new_dir)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect::<Vec<_>>();

  let reply = files
    .iter()
    .find(|f| std::fs::read(f).unwrap() != input)
    .expect("reply not found");
  let reply_str = std::fs::read_to_string(reply).unwrap();

  assert!(reply_str.contains("text/plain"), "missing text/plain part");
  assert!(reply_str.contains("text/html"), "missing text/html part");
  assert!(reply_str.contains("<h1>foo</h1>"), "missing rendered HTML");
}

#[test]
fn markdown_table() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(
    test.path(),
    "#!/bin/sh\ncat > /dev/null\nprintf '| foo | bar |\\n| --- | --- |\\n| baz | qux |\\n'\n",
  );
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db_str = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions_str = sessions.to_str().unwrap();
  let input = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
  let test = test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(input)
    .success();

  let new_dir = test.path().join("new");
  let files = std::fs::read_dir(&new_dir)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect::<Vec<_>>();

  let reply = files
    .iter()
    .find(|f| std::fs::read(f).unwrap() != input)
    .expect("reply not found");
  let reply_str = std::fs::read_to_string(reply).unwrap();

  assert!(reply_str.contains("<table>"), "missing table HTML");
  assert!(reply_str.contains("<td>baz</td>"), "missing table cell");
}

#[test]
fn agent_failure() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let claude = write_claude(test.path(), "#!/bin/sh\ncat > /dev/null\nexit 1\n");
  let dir = test.path().to_str().unwrap().to_string();
  let db = test.path().join("db.redb");
  let db_str = db.to_str().unwrap();
  let sessions = test.path().join("sessions");
  let sessions_str = sessions.to_str().unwrap();
  test
    .args([
      "mail",
      "--dir",
      &dir,
      "--sendmail",
      &sendmail,
      "--db",
      db_str,
      "--claude",
      &claude,
      "--session-dir",
      sessions_str,
    ])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz")
    .stderr_regex("error: agent exited with .*\n.*")
    .failure();
}
