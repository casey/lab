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

fn write_sendmail(dir: &std::path::Path, script: &str) -> String {
  use std::os::unix::fs::PermissionsExt;
  let path = dir.join("sendmail");
  std::fs::write(&path, script).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path.to_str().unwrap().to_string()
}

#[test]
fn missing_sender() {
  let sendmail = find_in_path("true");
  let test = Test::new();
  let dir = test.path().to_str().unwrap().to_string();
  test
    .args(["mail", "--dir", &dir, "--sendmail", &sendmail])
    .stdin(b"From: \r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz")
    .stderr("error: message has no sender\n")
    .failure();
}

#[test]
fn missing_message_id() {
  let sendmail = find_in_path("true");
  let test = Test::new();
  let dir = test.path().to_str().unwrap().to_string();
  test
    .args(["mail", "--dir", &dir, "--sendmail", &sendmail])
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
fn saves_incoming_and_reply() {
  let test = Test::new();
  let sendmail = write_sendmail(test.path(), "#!/bin/sh\ncat > /dev/null\n");
  let dir = test.path().to_str().unwrap().to_string();
  let input = b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\nContent-Type: text/plain\r\n\r\nbaz";
  let test = test
    .args(["mail", "--dir", &dir, "--sendmail", &sendmail])
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
  let dir = test.path().to_str().unwrap().to_string();
  let test = test
    .args(["mail", "--dir", &dir, "--sendmail", &sendmail])
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
  let dir = test.path().to_str().unwrap().to_string();
  test
    .args(["mail", "--dir", &dir, "--sendmail", &sendmail])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: failed to send reply\n.*")
    .failure();
}

#[test]
fn sendmail_not_found() {
  let test = Test::new();
  let dir = test.path().to_str().unwrap().to_string();
  test
    .args(["mail", "--dir", &dir, "--sendmail", "/nonexistent"])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: failed to send reply\n.*")
    .failure();
}

#[test]
fn unwritable_dir() {
  let sendmail = find_in_path("true");
  Test::new()
    .args(["mail", "--dir", "/proc/foo", "--sendmail", &sendmail])
    .stdin(b"From: foo@bar.com\r\nMessage-ID: <foo@bar>\r\n\r\nbaz")
    .stderr_regex("error: I/O error at `/proc/foo/cur`\n.*")
    .status(75);
}
