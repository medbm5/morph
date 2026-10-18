//! `morph` — every ring operation, from a shell.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "morph",
    version = morph_core::VERSION,
    about = "Metamorphosis — convert and edit files without touching the original",
    long_about = None
)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    Ok(())
}
