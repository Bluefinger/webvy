use std::path::{Path, PathBuf};

use bevy_tasks::{IoTaskPool, Task};
use log::trace;
use smol::{
    fs::{read_dir, read_to_string},
    stream::{iter, StreamExt},
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

pub async fn read_from_directory(
    path: impl AsRef<Path>,
) -> std::io::Result<Vec<(PathBuf, String)>> {
    let io = IoTaskPool::get();

    // Concurrently obtain files
    let tasks = find_all_files_in_directory(path.as_ref())
        .await?
        .into_iter()
        .map(|file| io.spawn(read_file(file)))
        .collect::<Vec<Task<std::io::Result<(PathBuf, String)>>>>();

    iter(tasks.into_iter())
        .then(|task| task)
        .try_collect()
        .await
}

async fn read_file(file: PathBuf) -> std::io::Result<(PathBuf, String)> {
    trace!("Reading {} from file", file.display());
    read_to_string(file.as_path())
        .await
        .map(move |body| (file, body))
}
