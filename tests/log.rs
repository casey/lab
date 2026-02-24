use super::*;

#[test]
fn non_json() {
  Test::new()
    .args(["log"])
    .stdin("foo")
    .stderr_regex(".*failed to parse JSON.*")
    .failure();
}
