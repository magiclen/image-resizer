use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, anyhow};

static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Returns the output subtree to skip while walking the input directory.
pub(super) fn excluded_output_root(
    input_root: &Path,
    output_root: Option<&Path>,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(output_root) = output_root else {
        return Ok(None);
    };

    let input = input_root.canonicalize().with_context(|| anyhow!("{input_root:?}"))?;
    let output = output_root.canonicalize().with_context(|| anyhow!("{output_root:?}"))?;

    if input == output {
        return Ok(None);
    }

    if input.starts_with(&output) {
        return Err(anyhow!("{output_root:?} is a parent directory of {input_root:?}."));
    }

    // WalkDir keeps the spelling of the input path, including any parent symlinks.
    Ok(output.strip_prefix(&input).ok().map(|relative| input_root.join(relative)))
}

fn create_temporary_file(directory: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            directory.join(format!(".{}-{}-{counter}.tmp", env!("CARGO_PKG_NAME"), process::id()));

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
    }
}

/// Writes through a new sibling file and renames it into place after the write succeeds, so an interrupted write cannot destroy the original image.
pub(super) fn write_atomically(output_path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let permissions = match fs::symlink_metadata(output_path) {
        // The rename below would replace the link itself rather than write through it. Callers are expected to have settled this already, so this is the last-line guard.
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(anyhow!("{output_path:?} is a symbolic link."));
        },
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).with_context(|| anyhow!("{output_path:?}")),
    };

    let directory = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (temp_path, mut file) =
        create_temporary_file(directory).with_context(|| anyhow!("{output_path:?}"))?;

    let result = (|| {
        // Apply the original permissions before any image data is written.
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }

        file.write_all(data)?;
        // The data must reach the disk before the rename, or a crash could leave an empty file in place of the original image.
        file.sync_data()?;
        drop(file);

        fs::rename(&temp_path, output_path)
    })();

    if result.is_err() {
        // create_new succeeded, so only this call owns the temporary file being removed.
        let _ = fs::remove_file(&temp_path);
    }

    result.with_context(|| anyhow!("{output_path:?} (temporary file: {temp_path:?})"))
}
