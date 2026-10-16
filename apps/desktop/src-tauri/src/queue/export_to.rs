//! Saving a copy of a finished file to a path the user chose, and the lookups behind "Open in
//! Word" (docs/protocol.md: `export_to`, `check_outputs`, `open_output`).
//!
//! The engine writes into a hidden staging folder next to the destination under the final file
//! name, so the Markdown links to the figure folder are right, and the result is then moved into
//! place. A rename replaces an existing file in one step, so a failed export never leaves a
//! half-written file where the user's file was.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::*;
use crate::contract::OutputStatus;

impl Queue {
    pub async fn export_to(
        &self,
        file_id: &str,
        format: ExportFormat,
        dest_path: &str,
        overwrite: bool,
    ) -> Result<OutputFile> {
        let row = self.row(file_id)?;
        if row.status != QueueStatus::Done {
            return Err(
                AppError::invalid("Only a finished file can be exported.").with_reason("not_done")
            );
        }
        let dest = validate_dest(format, dest_path)?;
        let assets = assets_dir_for(&dest);
        let taken = dest.exists() || (format == ExportFormat::Markdown && assets.exists());
        if taken && !overwrite {
            return Err(
                AppError::new(AppErrorCode::Io, "A file with that name already exists.")
                    .with_reason(ALREADY_EXISTS),
            );
        }
        let document = self.ensure_document(&row).await?;

        let parent = dest.parent().expect("validated").to_path_buf();
        let stage = parent.join(format!(
            ".opendoc-tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&stage)?;
        let result = self
            .export_into_stage(&document, format, &dest, &assets, &stage)
            .await;
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    async fn export_into_stage(
        &self,
        document: &Path,
        format: ExportFormat,
        dest: &Path,
        assets: &Path,
        stage: &Path,
    ) -> Result<OutputFile> {
        let staged = stage.join(dest.file_name().expect("validated"));
        match format {
            ExportFormat::Docx => {
                self.svc
                    .supervisor
                    .call(
                        "export.docx",
                        json!({"document": path_str(document), "outputPath": path_str(&staged)}),
                        None,
                    )
                    .await
                    .map_err(|e| e.to_app_error())?;
                std::fs::rename(&staged, dest)?;
                Ok(OutputFile {
                    format,
                    path: path_str(dest),
                    assets_dir: None,
                })
            }
            ExportFormat::Markdown => {
                let staged_assets = stage.join(assets.file_name().expect("validated"));
                self.svc
                    .supervisor
                    .call(
                        "export.markdown",
                        json!({
                            "document": path_str(document),
                            "outputPath": path_str(&staged),
                            "assetsDir": path_str(&staged_assets),
                        }),
                        None,
                    )
                    .await
                    .map_err(|e| e.to_app_error())?;
                let has_assets = staged_assets.is_dir();
                if has_assets {
                    move_assets(&staged_assets, assets)?;
                }
                std::fs::rename(&staged, dest)?;
                Ok(OutputFile {
                    format,
                    path: path_str(dest),
                    assets_dir: has_assets.then(|| path_str(assets)),
                })
            }
        }
    }

    /// The saved outputs of a file and whether each one is still on disk.
    pub fn check_outputs(&self, file_id: &str) -> Result<Vec<OutputStatus>> {
        Ok(self
            .row(file_id)?
            .outputs
            .into_iter()
            .map(|o| OutputStatus {
                exists: Path::new(&o.path).is_file(),
                format: o.format,
                path: o.path,
            })
            .collect())
    }

    /// The path, if it is an output the queue wrote and the file is still there.
    pub fn known_output(&self, path: &str) -> Result<PathBuf> {
        let wanted = Path::new(path);
        let known = self
            .db
            .all()?
            .iter()
            .flat_map(|r| r.outputs.iter())
            .any(|o| Path::new(&o.path) == wanted);
        if !known {
            return Err(
                AppError::invalid("That is not a saved output.").with_reason("not_an_output")
            );
        }
        if !wanted.is_file() {
            return Err(AppError::not_found("The file is gone.").with_reason("file_missing"));
        }
        Ok(wanted.to_path_buf())
    }
}

/// An absolute path to a file name with the right extension in a folder that exists.
fn validate_dest(format: ExportFormat, dest_path: &str) -> Result<PathBuf> {
    let dest = PathBuf::from(dest_path);
    let bad =
        |why: &'static str| AppError::invalid("That is not a usable save path.").with_reason(why);
    if !dest.is_absolute() || dest.file_name().is_none() {
        return Err(bad("bad_path"));
    }
    let ext = dest
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ok = match format {
        ExportFormat::Docx => ext == "docx",
        ExportFormat::Markdown => ext == "md" || ext == "markdown",
    };
    if !ok {
        return Err(bad("bad_extension"));
    }
    if dest.is_dir() {
        return Err(bad("is_directory"));
    }
    if !dest.parent().is_some_and(Path::is_dir) {
        return Err(bad("no_folder"));
    }
    Ok(dest)
}

/// `name.md` keeps its figures in `name_assets`, as the conversion does.
fn assets_dir_for(dest: &Path) -> PathBuf {
    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string());
    dest.with_file_name(format!("{stem}_assets"))
}

/// Moves the staged figures into the final folder. An existing folder is kept and its files of
/// the same name replaced, so nothing the user put there is deleted.
fn move_assets(staged: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        std::fs::rename(staged, target)?;
        return Ok(());
    }
    for entry in std::fs::read_dir(staged)?.flatten() {
        std::fs::rename(entry.path(), target.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn destinations_need_an_absolute_path_the_right_extension_and_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("a.DOCX");
        assert_eq!(
            validate_dest(ExportFormat::Docx, ok.to_str().unwrap()).unwrap(),
            ok
        );
        let md = dir.path().join("a.md");
        assert!(validate_dest(ExportFormat::Markdown, md.to_str().unwrap()).is_ok());

        let reason = |format, path: &str| validate_dest(format, path).unwrap_err().reason;
        assert_eq!(
            reason(ExportFormat::Docx, "relative.docx").as_deref(),
            Some("bad_path")
        );
        assert_eq!(
            reason(ExportFormat::Docx, md.to_str().unwrap()).as_deref(),
            Some("bad_extension")
        );
        assert_eq!(
            reason(ExportFormat::Markdown, ok.to_str().unwrap()).as_deref(),
            Some("bad_extension")
        );
        let missing = dir.path().join("nope").join("a.docx");
        assert_eq!(
            reason(ExportFormat::Docx, missing.to_str().unwrap()).as_deref(),
            Some("no_folder")
        );
        let folder = dir.path().join("x.docx");
        fs::create_dir(&folder).unwrap();
        assert_eq!(
            reason(ExportFormat::Docx, folder.to_str().unwrap()).as_deref(),
            Some("is_directory")
        );
    }

    #[test]
    fn figures_follow_the_markdown_name() {
        assert_eq!(
            assets_dir_for(Path::new("/x/My notes.md")),
            Path::new("/x/My notes_assets")
        );
    }

    #[test]
    fn existing_figure_folders_are_merged_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let staged = dir.path().join("staged");
        let target = dir.path().join("t_assets");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(staged.join("p0.png"), "new").unwrap();
        fs::write(target.join("p0.png"), "old").unwrap();
        fs::write(target.join("mine.txt"), "keep").unwrap();
        move_assets(&staged, &target).unwrap();
        assert_eq!(fs::read_to_string(target.join("p0.png")).unwrap(), "new");
        assert_eq!(fs::read_to_string(target.join("mine.txt")).unwrap(), "keep");

        let fresh = dir.path().join("f_assets");
        let staged2 = dir.path().join("staged2");
        fs::create_dir_all(&staged2).unwrap();
        fs::write(staged2.join("a.png"), "x").unwrap();
        move_assets(&staged2, &fresh).unwrap();
        assert!(fresh.join("a.png").is_file());
    }
}
