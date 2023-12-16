use std::path::Path;

use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use miette::IntoDiagnostic;
use ruplacer::{Console, FilePatcher, Query};
use tokio::io::{AsyncBufReadExt, BufReader};

fn init_logging() {
    #[cfg(debug_assertions)]
    std::env::set_var("RUST_LOG", "debug");

    #[cfg(not(debug_assertions))]
    std::env::set_var("RUST_LOG", "info");

    tracing_subscriber::fmt::init();
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    init_logging();

    ProjectType::select("Choose a type of project you want to create")?
        .proceed()
        .await
}

trait Selectable: std::fmt::Display + Sized {
    fn all() -> &'static [Self];
    fn as_str(&self) -> &'static str;
    fn select(prompt: impl AsRef<str>) -> miette::Result<&'static Self> {
        Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt.as_ref())
            .items(Self::all())
            .interact()
            .map(|ind| &Self::all()[ind])
            .into_diagnostic()
    }
}

macro_rules! selectable_enum {
    ($name:ident {
        $($var:ident = $str:literal),*
        $(,)*
    }) => {
        enum $name {
            $($var),*
        }

        impl Selectable for $name {
            fn all() -> &'static [Self] {
                &[$(Self::$var),*]
            }

            fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$var => $str),*
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }
    };
}

selectable_enum!(ProjectType {
    Rust = "Rust"
});

impl ProjectType {
    async fn proceed(&self) -> miette::Result<()> {
        RustTemplate::select("Choose on of the Rust project templates")?
            .proceed()
            .await
    }
}

selectable_enum!(RustTemplate {
    NixCargoIntegration = "nix-cargo-integration"
});

impl RustTemplate {
    async fn proceed(&self) -> miette::Result<()> {
        NixCargoIntegrationTemplate::select("Choose one of the nix-cargo-integration templates")?
            .proceed()
            .await
    }
}

selectable_enum!(NixCargoIntegrationTemplate {
    CrossCompileWasm = "cross-compile-wasm",
    SimpleCrate = "simple-crate",
    SimpleWorkspace = "simple-workspace"
});

impl NixCargoIntegrationTemplate {
    async fn proceed(&self) -> miette::Result<()> {
        let project_path: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Provide the name of your project (can be provided with parent dir path e.g. 'parent/project-name')")
            .interact_text()
            .into_diagnostic()?;
        let project_name = project_path.split('/').last().unwrap_or(&project_path);

        let current_dir = std::env::current_dir().into_diagnostic()?;
        let project_dir = current_dir.join(&project_path);

        if !project_dir.exists() {
            std::fs::create_dir_all(&project_dir).into_diagnostic()?;
        }

        std::env::set_current_dir(&project_dir).into_diagnostic()?;

        NixFlakeTemplateInfo::new("github", "yusdacra/nix-cargo-integration", self.as_str())
            .init()
            .await?;

        let console = Console::new();

        let replace_names_in_files =
            |paths: &[&str], from: String, to: String| -> miette::Result<()> {
                for file_name in paths {
                    if let Some(file_patcher) = FilePatcher::new(
                        &console,
                        Path::new(file_name),
                        &Query::Substring(from.to_string(), to.to_string()),
                    )
                    .map_err(|err| miette::miette!("{}", err))?
                    {
                        file_patcher
                            .run()
                            .map_err(|err| miette::miette!("{}", err))?;
                    }
                }

                Ok(())
            };

        const CRATE_REPLACEMENT_FILES: &[&str] =
            &["Cargo.toml", "Cargo.lock", "crates.nix", "flake.nix"];

        match self {
            NixCargoIntegrationTemplate::CrossCompileWasm => replace_names_in_files(
                CRATE_REPLACEMENT_FILES,
                "cross-compile".into(),
                project_name.into(),
            ),
            NixCargoIntegrationTemplate::SimpleCrate => {
                replace_names_in_files(
                    CRATE_REPLACEMENT_FILES,
                    "my-crate".into(),
                    project_name.into(),
                )?;
                replace_names_in_files(&["crates.nix"], "simple".into(), project_name.into())
            }
            NixCargoIntegrationTemplate::SimpleWorkspace => {
                let bin_crate_name: String = Input::new()
                    .with_prompt("Provide a name for the bin crate")
                    .interact_text()
                    .into_diagnostic()?;
                let lib_crate_name: String = Input::new()
                    .with_prompt("Provide a name for the lib crate")
                    .interact_text()
                    .into_diagnostic()?;

                replace_names_in_files(&["flake.nix"], "my-project".into(), project_name.into())?;
                replace_names_in_files(
                    &[
                        "flake.nix",
                        "Cargo.toml",
                        "Cargo.lock",
                        "my-crate/Cargo.toml",
                    ],
                    "my-crate".into(),
                    bin_crate_name.clone(),
                )?;
                replace_names_in_files(
                    &[
                        "flake.nix",
                        "Cargo.toml",
                        "Cargo.lock",
                        "my-other-crate/Cargo.toml",
                    ],
                    "my-other-crate".into(),
                    lib_crate_name.clone(),
                )?;

                tracing::info!("{} -> {}", "my-crate".red(), bin_crate_name.green());
                tokio::fs::rename("my-crate", bin_crate_name)
                    .await
                    .into_diagnostic()?;

                tracing::info!("{} -> {}", "my-other-crate".red(), lib_crate_name.green());
                tokio::fs::rename("my-other-crate", lib_crate_name)
                    .await
                    .into_diagnostic()?;

                Ok(())
            }
        }?;

        let _envrc_file = tokio::fs::File::create(".envrc").await.into_diagnostic()?;
        tokio::fs::write(".envrc", "use flake .")
            .await
            .into_diagnostic()?;

        let _gitignore_file = tokio::fs::File::create(".gitignore")
            .await
            .into_diagnostic()?;
        tokio::fs::write(".gitignore", "target/\n.direnv/")
            .await
            .into_diagnostic()?;

        Ok(())
    }
}

struct NixFlakeTemplateInfo {
    host: &'static str,
    repo: &'static str,
    derivation: &'static str,
}

impl NixFlakeTemplateInfo {
    const fn new(host: &'static str, repo: &'static str, derivation: &'static str) -> Self {
        Self {
            host,
            repo,
            derivation,
        }
    }

    async fn init(&self) -> miette::Result<()> {
        let mut child = tokio::process::Command::new("nix")
            .args([
                "flake",
                "init",
                "-t",
                &format!("{}:{}#{}", self.host, self.repo, self.derivation),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .into_diagnostic()?;

        let stdout = child
            .stdout
            .take()
            .ok_or(miette::miette!("No stdout handle"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(miette::miette!("No stderr handle"))?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        tracing::info!("Running nix...");

        loop {
            tokio::select! {
                result = stdout_reader.next_line() => {
                    if let Some(line) = result.into_diagnostic()? {
                        tracing::error!("{}", format!("{} {line}", "[nix]".red()));
                    }
                }
                result = stderr_reader.next_line() => {
                    if let Some(line) = result.into_diagnostic()? {
                        tracing::info!("{}", format!("{} {line}", "[nix]".green()));
                    }
                }
                result = child.wait() => {
                    let exit_status = result.into_diagnostic()?;

                    if !exit_status.success() {
                        return Err(miette::miette!("nix failed to execute: {}", exit_status))
                    }

                    return Ok(())
                }
            };
        }
    }
}
