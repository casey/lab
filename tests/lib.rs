use {
  self::{expected::Expected, test::Test},
  regex::Regex,
  std::{
    io::Write,
    process::{Command, Stdio},
    str,
  },
};

mod chat;
mod expected;
mod log;
mod mail;
mod test;
