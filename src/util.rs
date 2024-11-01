use std::path::{Path, PathBuf};

use miette::IntoDiagnostic;
use ruplacer::{Console, FilePatcher, Query};
use tokio::io::AsyncWriteExt;

pub fn name_or_dir_name(name: Option<String>, path: impl AsRef<Path>) -> miette::Result<String> {
    Ok(match name {
        Some(name) => name,
        None => {
            let path = path.as_ref();
            path.file_name()
                .ok_or_else(|| miette::miette!("Failed to get directory name from {path:?}"))?
                .to_str()
                .ok_or_else(|| miette::miette!("Failed to convert directory name to string"))?
                .to_string()
        }
    })
}

pub trait PathExt {
    async fn create_dir_and_cd(&self) -> miette::Result<()>;

    fn patch_file(&self, console: &Console, query: &Query) -> miette::Result<()>;

    fn patch_file_multiple<'a>(
        &self,
        console: &Console,
        queries: impl IntoIterator<Item = &'a Query>,
    ) -> miette::Result<()> {
        queries
            .into_iter()
            .try_for_each(|query| self.patch_file(console, query))
    }

    async fn create_file_with_str_content(&self, content: impl AsRef<str>) -> miette::Result<()>;
    async fn rename_path_entry(&self, to: impl AsRef<str>) -> miette::Result<()>;
}

impl<P> PathExt for P
where
    P: AsRef<Path>,
{
    async fn create_dir_and_cd(&self) -> miette::Result<()> {
        let path = self.as_ref();

        tracing::debug!("Creating dir {path:?}");

        tokio::fs::create_dir_all(path).await.into_diagnostic()?;

        tracing::debug!("Setting current directory to {path:?}");

        std::env::set_current_dir(path).into_diagnostic()?;

        Ok(())
    }

    fn patch_file(&self, console: &Console, query: &Query) -> miette::Result<()> {
        let path = self.as_ref();

        tracing::info!("Patching file {path:?}");

        FilePatcher::new(console, path, query)
            .map_err(|err| miette::miette!("{err}"))?
            .ok_or_else(|| miette::miette!("Failed to initializa a patcher for {path:?}"))?
            .run()
            .map_err(|err| miette::miette!("Failed to patch {path:?}: {err}"))
    }

    async fn create_file_with_str_content(&self, content: impl AsRef<str>) -> miette::Result<()> {
        let path = self.as_ref();

        tracing::info!("Creating file {path:?}");

        let content = content.as_ref();

        let mut file = tokio::fs::File::create(path).await.into_diagnostic()?;
        file.write_all(content.as_bytes()).await.into_diagnostic()
    }

    async fn rename_path_entry(&self, to: impl AsRef<str>) -> miette::Result<()> {
        let path = self.as_ref();
        let to = to.as_ref();

        let parent = path.parent();

        let to = match parent {
            Some(parent) => parent.join(to),
            None => PathBuf::from(to),
        };

        tracing::info!("Renaming {path:?} to {to:?}");

        tokio::fs::rename(path, to).await.into_diagnostic()
    }
}

pub trait StrQuery {
    fn simple_query_to(&self, to: impl AsRef<str>) -> Query;

    fn as_path(&self) -> &Path;
}

impl<S> StrQuery for S
where
    S: AsRef<str>,
{
    fn simple_query_to(&self, to: impl AsRef<str>) -> Query {
        Query::simple(self.as_ref(), to.as_ref())
    }

    fn as_path(&self) -> &Path {
        self.as_ref().as_ref()
    }
}
