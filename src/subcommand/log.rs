use super::*;

#[derive(clap::Args)]
pub(crate) struct Log {}

impl Log {
  pub(crate) fn run(self) -> Result {
    let mut input = String::new();
    io::stdin()
      .read_to_string(&mut input)
      .context(error::Stdin)?;
    ::log::info!("{input}");
    Ok(())
  }
}
