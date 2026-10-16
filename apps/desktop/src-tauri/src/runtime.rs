//! The llama-server runtime on disk (M1-7): where a build lives, and extracting its archives.
//!
//! Windows builds are zip files and macOS builds are tar.gz. Every archive of a build is
//! extracted into the same folder (the CUDA build needs its cudart archive next to it).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::contract::AppError;
use crate::manifest::Manifest;

pub const SERVER_NAME: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

const MARKER: &str = ".installed";

/// `<runtime>/llama-<build>-<manifest key>`, for example `llama-b11669-windows-x64-cuda`.
pub fn build_dir(runtime_root: &Path, manifest: &Manifest, build_key: &str) -> PathBuf {
    runtime_root.join(format!(
        "llama-{}-{build_key}",
        manifest.runtimes.llama_server.build
    ))
}

/// Path to `llama-server` when the build in `dir` is completely installed.
pub fn installed_server(dir: &Path, manifest: &Manifest) -> Option<PathBuf> {
    let marker = fs::read_to_string(dir.join(MARKER)).ok()?;
    if marker.trim() != manifest.runtimes.llama_server.build {
        return None;
    }
    find_server(dir)
}

/// Looks for the server binary in `dir` and one or two levels below it (the macOS archive has a
/// top-level folder).
pub fn find_server(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: u32) -> Option<PathBuf> {
        let mut subdirs = Vec::new();
        for entry in fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            } else if path.file_name().is_some_and(|n| n == SERVER_NAME) {
                return Some(path);
            }
        }
        if depth == 0 {
            return None;
        }
        subdirs.into_iter().find_map(|d| walk(&d, depth - 1))
    }
    walk(dir, 2)
}

pub fn mark_installed(dir: &Path, manifest: &Manifest) -> io::Result<()> {
    fs::write(
        dir.join(MARKER),
        format!("{}\n", manifest.runtimes.llama_server.build),
    )
}

pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dest)?;
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else {
        Err(AppError::internal("Unknown archive type.").with_detail(name))
    }
}

fn extract_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        crate::contract::AppErrorCode::Io,
        "The runtime archive could not be unpacked.",
    )
    .with_reason("extract_failed")
    .with_detail(error.to_string())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), AppError> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(extract_failed)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(extract_failed)?;
        // `enclosed_name` is None for names that would escape `dest`; skip those.
        let Some(relative) = entry.enclosed_name() else {
            tracing::warn!("skipped an archive entry with an unsafe path");
            continue;
        };
        let target = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&target)?;
        io::copy(&mut entry, &mut out)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), AppError> {
    let file = fs::File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    tar.set_preserve_permissions(true);
    // `unpack` refuses entries that would land outside `dest`.
    tar.unpack(dest).map_err(extract_failed)
}

/// Makes the server binary executable (a no-op on Windows).
pub fn make_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Extracts every archive into `dir`, checks that `llama-server` is there, makes it executable
/// and writes the marker. Blocking.
pub fn install_archives(
    archives: &[PathBuf],
    dir: &Path,
    manifest: &Manifest,
) -> Result<PathBuf, AppError> {
    // Start from a clean folder so a half-extracted earlier attempt cannot linger.
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    for archive in archives {
        extract_archive(archive, dir)?;
    }
    let server = find_server(dir).ok_or_else(|| {
        AppError::new(
            crate::contract::AppErrorCode::Io,
            "llama-server is missing from the runtime archive.",
        )
        .with_reason("extract_failed")
    })?;
    make_executable(&server)?;
    mark_installed(dir, manifest)?;
    Ok(server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn zip_archives_are_merged_into_one_folder() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.zip");
        let cudart = dir.path().join("cudart.zip");
        write_zip(&main, &[(SERVER_NAME, b"server"), ("ggml.dll", b"ggml")]);
        write_zip(&cudart, &[("cudart64_13.dll", b"cuda")]);
        let out = dir.path().join("out");
        let manifest = Manifest::embedded();

        let server = install_archives(&[main, cudart], &out, manifest).unwrap();

        assert_eq!(server, out.join(SERVER_NAME));
        assert!(out.join("cudart64_13.dll").exists());
        assert!(installed_server(&out, manifest).is_some());
    }

    #[test]
    fn tar_gz_archives_with_a_top_level_folder_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("llama.tar.gz");
        let inner = format!("llama-b1/{SERVER_NAME}");
        write_tar_gz(
            &archive,
            &[(&inner, b"server"), ("llama-b1/libggml.dylib", b"x")],
        );
        let out = dir.path().join("out");

        let server = install_archives(&[archive], &out, Manifest::embedded()).unwrap();

        assert_eq!(server, out.join("llama-b1").join(SERVER_NAME));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&server).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "the server is executable");
        }
    }

    #[test]
    fn entries_that_escape_the_folder_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        write_zip(&archive, &[("../evil.txt", b"x"), (SERVER_NAME, b"server")]);
        let out = dir.path().join("deep").join("out");
        install_archives(&[archive], &out, Manifest::embedded()).unwrap();
        assert!(!dir.path().join("deep").join("evil.txt").exists());
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn an_archive_without_the_server_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("empty.zip");
        write_zip(&archive, &[("readme.txt", b"x")]);
        let error = install_archives(&[archive], &dir.path().join("out"), Manifest::embedded())
            .unwrap_err();
        assert_eq!(error.reason.as_deref(), Some("extract_failed"));
    }

    #[test]
    fn an_unmarked_folder_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(SERVER_NAME), b"x").unwrap();
        assert!(installed_server(dir.path(), Manifest::embedded()).is_none());
    }

    #[test]
    fn build_folders_are_named_by_build_and_key() {
        let manifest = Manifest::embedded();
        let dir = build_dir(Path::new("/r"), manifest, "windows-x64-cuda");
        assert_eq!(dir, Path::new("/r").join("llama-b11669-windows-x64-cuda"));
    }
}
