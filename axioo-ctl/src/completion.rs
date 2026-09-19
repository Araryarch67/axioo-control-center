//! `axioo-ctl completion`: shell completions via `clap_complete` (stdout).

use clap::CommandFactory;
use clap_complete::{generate, shells};
use std::io;

use crate::Cli;

pub fn run(shell: &str) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    let mut buf = io::stdout();
    match shell.to_ascii_lowercase().as_str() {
        "bash" => generate(shells::Bash, &mut cmd, &name, &mut buf),
        "fish" => generate(shells::Fish, &mut cmd, &name, &mut buf),
        "zsh" => generate(shells::Zsh, &mut cmd, &name, &mut buf),
        "powershell" | "ps1" => generate(shells::PowerShell, &mut cmd, &name, &mut buf),
        "elvish" => generate(shells::Elvish, &mut cmd, &name, &mut buf),
        other => {
            eprintln!("error: unknown shell '{other}' (pilih: bash fish zsh powershell elvish)");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_completion_mentions_subcommands() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        generate(shells::Bash, &mut cmd, "axioo-ctl", &mut buf);
        let text = String::from_utf8(buf).unwrap();
        for sub in [
            "probe", "monitor", "fan", "profile", "kbd", "battery", "gpu",
        ] {
            assert!(text.contains(sub), "completion lacks '{sub}'");
        }
    }
}
