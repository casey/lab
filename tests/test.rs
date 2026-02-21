use {super::*, pretty_assertions::assert_eq};

pub(crate) struct Test {
  args: Vec<String>,
  stderr: Expected,
  stdin: Option<Vec<u8>>,
  stdout: Expected,
  tempdir: tempfile::TempDir,
}

impl Test {
  pub(crate) fn new() -> Self {
    Self {
      args: Vec::new(),
      stderr: Expected::Empty,
      stdin: None,
      stdout: Expected::Empty,
      tempdir: tempfile::TempDir::new().unwrap(),
    }
  }

  pub(crate) fn args(mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
    assert!(self.args.is_empty());
    for arg in args {
      self.args.push(arg.as_ref().into());
    }
    self
  }

  pub(crate) fn path(&self) -> &std::path::Path {
    self.tempdir.path()
  }

  pub(crate) fn stderr(mut self, stderr: &str) -> Self {
    assert!(matches!(self.stderr, Expected::Empty));
    self.stderr = Expected::String(stderr.into());
    self
  }

  pub(crate) fn stderr_regex(mut self, pattern: &str) -> Self {
    assert!(matches!(self.stderr, Expected::Empty));
    self.stderr = Expected::regex(pattern);
    self
  }

  pub(crate) fn stdin(mut self, stdin: impl AsRef<[u8]>) -> Self {
    assert!(self.stdin.is_none());
    self.stdin = Some(stdin.as_ref().to_vec());
    self
  }

  pub(crate) fn stdout_regex(mut self, pattern: &str) -> Self {
    assert!(matches!(self.stdout, Expected::Empty));
    self.stdout = Expected::regex(pattern);
    self
  }

  #[track_caller]
  pub(crate) fn status(self, code: i32) -> Self {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lab"));

    command.args(&self.args);

    let child = command
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .unwrap();

    if let Some(stdin) = &self.stdin {
      child.stdin.as_ref().unwrap().write_all(stdin).unwrap();
    }

    let output = child.wait_with_output().unwrap();

    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();

    if code == 0 && !output.status.success() {
      eprintln!("{stderr}");
      panic!("command failed with {}", output.status);
    }

    assert!(
      !(code != 0 && output.status.success()),
      "command unexpectedly succeeded",
    );

    assert_eq!(output.status.code(), Some(code));

    self.stderr.check(stderr, "stderr");
    self.stdout.check(stdout, "stdout");

    Self {
      args: Vec::new(),
      stderr: Expected::Empty,
      stdin: None,
      stdout: Expected::Empty,
      tempdir: self.tempdir,
    }
  }

  #[track_caller]
  pub(crate) fn success(self) -> Self {
    self.status(0)
  }

  #[track_caller]
  pub(crate) fn failure(self) -> Self {
    self.status(1)
  }
}
