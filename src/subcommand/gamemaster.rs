use super::*;

const SESSION_DIR: &str = "/root/sessions";

const SYSTEM_PROMPT: &str = "\
You are the gamemaster for a web-based text game at /root/src/game. The game \
is rendered on a 100x100 character grid served over WebSocket. The client is \
fully general: it displays whatever the server sends. All game logic is \
server-side in Rust.\n\
\n\
The long-term vision is a multimodal game in the spirit of Frog Fractions: \
the roguelike is just the surface. Hidden within it should be entirely \
different games — puzzles, text adventures, minigames, genre shifts — \
discovered through exploration and surprising triggers. Build toward this \
over time. Each session, you might deepen the roguelike, add a hidden game \
within it, or plant the seeds for one.\n\
\n\
Each session, make exactly one focused change. Read the source code, review \
player logs with `journalctl -t game --since yesterday`, and choose what to \
work on. It could be a new feature, a hidden game, a balance tweak, a bug \
fix, or a quality-of-life improvement.\n\
\n\
After making your change:\n\
1. `cargo fmt && cargo test` in /root/src/game\n\
2. `cargo build --release` in /root/src/game\n\
3. `install -m 755 target/release/game /var/lib/game/game`\n\
4. `systemctl restart game`\n\
5. Commit your changes with a descriptive message\n\
6. Send Casey an email summarizing what you did and why, using \
`sendmail` as `Root <root@tulip.farm>` to `casey@rodarmor.com`";

const SESSION_NAME: &str = "gamemaster";

#[derive(clap::Args)]
pub(crate) struct Gamemaster {
  #[arg(long)]
  db: Option<PathBuf>,
  #[arg(long, default_value = "claude")]
  claude: PathBuf,
  #[arg(long)]
  reset: bool,
}

impl Gamemaster {
  pub(crate) fn run(self) -> Result {
    let db = self.db.clone().unwrap_or_else(db_path);

    if self.reset {
      return reset_session(&db, SESSION_NAME);
    }

    let (session, resume) = lookup_session(&db, SESSION_NAME)?;

    let prompt = "Review the game at /root/src/game and make one improvement. \
      Read the source, check `journalctl -t game --since yesterday` for player \
      activity, and decide what to work on.";

    let response = invoke_agent(
      &self.claude,
      Path::new(SESSION_DIR),
      &session,
      resume,
      prompt,
      Some(SYSTEM_PROMPT),
      false,
    )?;

    if !resume {
      save_session(&db, SESSION_NAME, &session)?;
    }

    let response = response.trim();

    notify::send(&format!("gamemaster: {response}"))
  }
}
