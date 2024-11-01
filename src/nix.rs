use std::{path::PathBuf, process::Stdio, time::Duration};

use indicatif::ProgressBar;
use miette::IntoDiagnostic;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

pub struct Nix {
    path: PathBuf,
}

impl Nix {
    pub fn locate() -> miette::Result<Self> {
        let path = which::which("nix").into_diagnostic()?;

        Ok(Self { path })
    }

    pub async fn init_template(&self, template: impl AsRef<str>) -> miette::Result<()> {
        let template = template.as_ref();

        let mut child = Command::new(&self.path)
            .args(["flake", "init", "-t", template])
            .stdout(Stdio::piped())
            .spawn()
            .into_diagnostic()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| miette::miette!("Failed to get handle to nix stdout!"))?;

        let progress_bar = ProgressBar::new_spinner();
        progress_bar.enable_steady_tick(Duration::from_millis(250));
        progress_bar.println(format!("Running 'nix flake init -t {template}'"));

        let mut reader = BufReader::new(stdout).lines();

        let handle = tokio::spawn(async move { child.wait().await });

        while let Some(line) = reader.next_line().await.into_diagnostic()? {
            progress_bar.set_message(line);
        }

        progress_bar.finish_and_clear();

        handle.await.into_diagnostic()?.into_diagnostic()?;

        Ok(())
    }
}
