use super::*;

#[test]
fn chat_help() {
  Test::new()
    .args(["chat", "--help"])
    .stdout_regex("Usage: lab chat.*")
    .success();
}

#[test]
fn chat_db_required() {
  Test::new()
    .args(["chat"])
    .stderr_regex(".*required.*--db.*")
    .status(2);
}
