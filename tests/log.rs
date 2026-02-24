use super::*;

#[test]
fn log() {
  Test::new()
    .args(["log"])
    .stdin(r#"{"foo":"bar"}"#)
    .success();
}
