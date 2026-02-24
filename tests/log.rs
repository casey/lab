use super::*;

#[test]
fn log() {
  if !std::path::Path::new("/run/systemd/journal/socket").exists() {
    return;
  }

  Test::new()
    .args(["log"])
    .stdin(r#"{"foo":"bar"}"#)
    .success();
}

#[test]
fn non_json() {
  Test::new()
    .args(["log"])
    .stdin("foo")
    .stderr_regex(".*failed to parse JSON.*")
    .failure();
}
