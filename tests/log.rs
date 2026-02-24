use super::*;

#[test]
fn log() {
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
