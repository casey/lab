use super::*;

#[test]
fn chat_help() {
  Test::new()
    .args(["chat", "--help"])
    .stdout_regex("Usage: lab chat.*")
    .success();
}
