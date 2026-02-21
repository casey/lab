use {
  self::{expected::Expected, test::Test},
  regex::Regex,
  std::{
    io::Write,
    process::{Command, Stdio},
    str,
  },
  tempfile,
};

mod expected;
mod mail;
mod test;
