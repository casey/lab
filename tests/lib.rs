use {
  self::{expected::Expected, test::Test},
  regex::Regex,
  std::{
    io::Write,
    process::{Command, Stdio},
    str,
  },
};

mod expected;
mod mail;
mod test;
