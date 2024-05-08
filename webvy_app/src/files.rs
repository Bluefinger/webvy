use std::path::{Path, PathBuf};

use futures_concurrency::concurrent_stream::{ConcurrentStream, IntoConcurrentStream};
use log::trace;
use smol::{
    fs::{read_dir, read_to_string},
    stream::StreamExt,
};

async fn find_all_files_in_directory(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    trace!("Reading directory: {}", path.display());
    let mut entry = read_dir(path).await?;

    let mut to_visit = Vec::new();

    while let Some(entry) = entry.try_next().await? {
        let path = entry.path();

        if path.is_dir() {
            let paths = Box::pin(find_all_files_in_directory(path.as_path())).await?;

            to_visit.extend(paths);
        } else if path.is_file() {
            trace!("Found: {}", path.display());
            to_visit.push(path);
        }
    }

    Ok(to_visit)
}

pub async fn read_all_from_directory(
    path: impl AsRef<Path>,
) -> Vec<std::io::Result<(PathBuf, String)>> {
    match find_all_files_in_directory(path.as_ref()).await {
        Ok(files) => files.into_co_stream().map(read_file).collect().await,
        Err(e) => vec![Err(e)],
    }
}

async fn read_file(file: PathBuf) -> std::io::Result<(PathBuf, String)> {
    trace!("Reading {} from file", file.display());
    read_to_string(file.as_path())
        .await
        .map(move |body| (file, body))
}
