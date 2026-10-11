use clap::Parser;
use std::io::Write;

/// LGTM code review command-line interface.
#[derive(Parser)]
#[command(version, about)]
struct Cli {}

/// Parse the given arguments and emit the greeting to `out`.
fn run(cli: Cli, mut out: impl Write) -> anyhow::Result<()> {
    let _ = cli;
    writeln!(out, "hello lgtm-cli!")?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run(cli, std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_the_exact_greeting() {
        let cli = Cli::parse_from(["lgtm-cli"]);
        let mut out = Vec::new();
        run(cli, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "hello lgtm-cli!\n");
    }
}
