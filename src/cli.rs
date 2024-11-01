use std::path::PathBuf;

use clap::{Parser, Subcommand};
use miette::IntoDiagnostic;
use ruplacer::Console;

use crate::{
    nix::Nix,
    util::{name_or_dir_name, PathExt, StrQuery},
};

/// A CLI tool that let's you get started on a new project
/// with fully set up nix flake devshell
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Choose a language to start a project with
    #[command(subcommand)]
    pub language: LanguageCommand,

    /// Quiet verbosity level
    #[arg(long)]
    pub quiet: bool,
    /// Debug verbosity level
    #[arg(long)]
    pub debug: bool,
    /// Trace verbosity level
    #[arg(long)]
    pub trace: bool,
}

#[derive(Subcommand)]
pub enum LanguageCommand {
    /// Commands for setting up a new Rust project
    Rust {
        #[command(subcommand)]
        command: RustCommand,
    },
}

impl LanguageCommand {
    pub async fn run(self, nix: Nix) -> miette::Result<()> {
        match self {
            LanguageCommand::Rust { command } => command.run(nix).await,
        }
    }
}

#[derive(Subcommand)]
pub enum RustCommand {
    Crate {
        /// Destionation path of the project
        path: PathBuf,
        /// Project name (defaults to the directory name from [path])
        #[arg(long)]
        project_name: Option<String>,
        /// Crate name (defaults to [project_name])
        #[arg(long)]
        name: Option<String>,
    },
    Workspace {
        /// Destination path of the project
        path: PathBuf,
        /// Name of the bin crate
        bin_name: String,
        /// Name of the lib crate (defaults to "{bin_crate}_core")
        lib_name: Option<String>,
        /// Name of the project (defaults to the directory name from [path])
        #[arg(long)]
        project_name: Option<String>,
    },
}

impl RustCommand {
    async fn run(self, nix: Nix) -> miette::Result<()> {
        const TEMPLATE_BASE: &str = "github:yusdacra/nix-cargo-integration";
        const CARGO_TOML_PATH: &str = "Cargo.toml";
        const FLAKE_PATH: &str = "flake.nix";
        const CRATES_FILE: &str = "crates.nix";
        const MY_CRATE: &str = "my-crate";

        let path = self.path().clone();

        path.create_dir_and_cd().await?;
        nix.init_template(format!("{TEMPLATE_BASE}#{}", self.template_name()))
            .await?;

        ".envrc"
            .create_file_with_str_content(
                [
                    "watch_file flake.nix",
                    "watch_file crates.nix",
                    "",
                    "rust-toolchain.toml",
                    "",
                    "use flake .",
                ]
                .join("\n"),
            )
            .await?;

        const IGNORE: [&str; 2] = ["target/", ".direnv/"];
        ".ignore"
            .create_file_with_str_content(IGNORE.join("\n"))
            .await?;
        ".gitignore"
            .create_file_with_str_content(IGNORE.join("\n"))
            .await?;

        "rust-toolchain.toml"
            .create_file_with_str_content(
                toml::to_string_pretty(&toml::toml! {
                    [toolchain]
                    channel = "nightly"
                    components = [
                      "rustc",
                      "rust-src",
                      "rust-std",
                      "rust-analysis",

                      "rustfmt",
                      "clippy",
                      "rust-analyzer",
                    ]
                })
                .into_diagnostic()?,
            )
            .await?;

        // For file patching
        let console = Console::new();

        match self {
            RustCommand::Crate {
                path,
                project_name,
                name,
            } => {
                let project_name = name_or_dir_name(project_name, &path)?;
                let name = name.unwrap_or(project_name.clone());

                // Replacement queries
                let my_crate_replace = MY_CRATE.simple_query_to(&name);
                let simple_replace = "simple".simple_query_to(&project_name);

                // flake.nix
                FLAKE_PATH.patch_file(&console, &my_crate_replace)?;

                // crates.nix
                CRATES_FILE.patch_file_multiple(&console, [&my_crate_replace, &simple_replace])?;

                // Cargo.toml
                let edition_replace = "2018".simple_query_to("2021");

                CARGO_TOML_PATH
                    .patch_file_multiple(&console, [&my_crate_replace, &edition_replace])?;
            }
            RustCommand::Workspace {
                path,
                bin_name,
                lib_name,
                project_name,
            } => {
                const MY_WORKSPACE_CRATE: &str = "my-workspace-crate";
                const MY_OTHER_WORKSPACE_CRATE: &str = "my-other-workspace-crate";

                let project_name = name_or_dir_name(project_name, &path)?;
                let lib_name = lib_name.unwrap_or_else(|| format!("{bin_name}_core"));

                let my_project_replace = "my-project".simple_query_to(&project_name);

                // flake.nix
                let my_crate_replace = MY_CRATE.simple_query_to(&bin_name);

                FLAKE_PATH
                    .patch_file_multiple(&console, [&my_project_replace, &my_crate_replace])?;

                // crates.nix
                let my_workspace_crate_replace = MY_WORKSPACE_CRATE.simple_query_to(&bin_name);
                let my_other_workspace_crate_replace =
                    MY_OTHER_WORKSPACE_CRATE.simple_query_to(&lib_name);

                CRATES_FILE.patch_file_multiple(
                    &console,
                    [
                        &my_project_replace,
                        &my_workspace_crate_replace,
                        &my_other_workspace_crate_replace,
                    ],
                )?;

                // Cargo.toml
                let members_replace =
                    format!("[{MY_WORKSPACE_CRATE:?}, {MY_OTHER_WORKSPACE_CRATE:?}]")
                        .simple_query_to(format!("[{bin_name:?}, {lib_name:?}]"));
                CARGO_TOML_PATH.patch_file(&console, &members_replace)?;

                // Bin crate
                MY_WORKSPACE_CRATE.rename_path_entry(&bin_name).await?;

                bin_name
                    .as_path()
                    .join(CARGO_TOML_PATH)
                    .patch_file(&console, &my_workspace_crate_replace)?;

                // Lib crate
                MY_OTHER_WORKSPACE_CRATE
                    .rename_path_entry(&lib_name)
                    .await?;

                lib_name
                    .as_path()
                    .join(CARGO_TOML_PATH)
                    .patch_file(&console, &my_other_workspace_crate_replace)?;
            }
        }

        tracing::info!(
            "Finished setting up your Rust project!\nYou can now cd into {path:?} to start hacking away!"
        );

        Ok(())
    }

    fn path(&self) -> &PathBuf {
        match self {
            RustCommand::Crate { path, .. } => path,
            RustCommand::Workspace { path, .. } => path,
        }
    }

    fn template_name(&self) -> &'static str {
        match self {
            RustCommand::Crate { .. } => "simple-crate",
            RustCommand::Workspace { .. } => "simple-workspace",
        }
    }
}
