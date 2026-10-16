//! Which files the queue accepts and what the outputs are called.
//!
//! Outputs are never overwritten: when a name is taken a number is added, " (2)", " (3)" and so
//! on. This is the only place that rule lives (docs/plans/v0.1.md, M3-4); the engine refuses to
//! overwrite as a second line of defence.

use std::path::{Path, PathBuf};

use crate::contract::{ExportFormat, FileKind};

const SUPPORTED: [&str; 10] = [
    "pdf", "png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp", "heic", "heif",
];

/// How deep a dropped folder is searched, so a link loop cannot run forever.
const MAX_DEPTH: usize = 32;

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn is_supported(path: &Path) -> bool {
    SUPPORTED.contains(&extension(path).as_str())
}

pub fn kind_of(path: &Path) -> FileKind {
    if extension(path) == "pdf" {
        FileKind::Pdf
    } else {
        FileKind::Image
    }
}

/// Expands dropped paths: files of supported types as they are, folders searched recursively
/// (in name order, so the queue order is stable). Anything else is skipped.
pub fn collect(paths: &[String]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&path, depth + 1, out);
            } else if kind.is_file() && is_supported(&path) {
                out.push(path);
            }
        }
    }

    let mut found = Vec::new();
    for raw in paths {
        let path = PathBuf::from(raw);
        if path.is_dir() {
            walk(&path, 0, &mut found);
        } else if path.is_file() && is_supported(&path) {
            found.push(path);
        }
    }
    found
}

/// Output names for one conversion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputPlan {
    pub docx: Option<PathBuf>,
    /// The Markdown file and its figure folder.
    pub markdown: Option<(PathBuf, PathBuf)>,
}

fn candidate(dir: &Path, stem: &str, number: u32, formats: &[ExportFormat]) -> OutputPlan {
    let base = if number <= 1 {
        stem.to_string()
    } else {
        format!("{stem} ({number})")
    };
    OutputPlan {
        docx: formats
            .contains(&ExportFormat::Docx)
            .then(|| dir.join(format!("{base}.docx"))),
        markdown: formats.contains(&ExportFormat::Markdown).then(|| {
            (
                dir.join(format!("{base}.md")),
                dir.join(format!("{base}_assets")),
            )
        }),
    }
}

fn is_free(plan: &OutputPlan) -> bool {
    plan.docx.as_ref().is_none_or(|p| !p.exists())
        && plan
            .markdown
            .as_ref()
            .is_none_or(|(md, assets)| !md.exists() && !assets.exists())
}

/// The first numbering (none, then 2, 3, ...) for which every requested output is free. All
/// formats of one conversion share the number, so they pair up: "scan (2).docx" goes with
/// "scan (2).md".
pub fn plan_outputs(dir: &Path, stem: &str, formats: &[ExportFormat]) -> OutputPlan {
    let mut number = 1;
    loop {
        let plan = candidate(dir, stem, number, formats);
        if is_free(&plan) {
            return plan;
        }
        number += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const BOTH: [ExportFormat; 2] = [ExportFormat::Docx, ExportFormat::Markdown];

    #[test]
    fn supported_types_are_matched_case_insensitively() {
        for name in [
            "a.pdf", "a.PDF", "b.Png", "c.jpeg", "d.TIF", "e.webp", "f.heic", "g.bmp",
        ] {
            assert!(is_supported(Path::new(name)), "{name}");
        }
        for name in ["a.docx", "b.txt", "noext", "c.pdf.bak"] {
            assert!(!is_supported(Path::new(name)), "{name}");
        }
        assert_eq!(kind_of(Path::new("a.PDF")), FileKind::Pdf);
        assert_eq!(kind_of(Path::new("a.png")), FileKind::Image);
    }

    #[test]
    fn folders_are_searched_recursively_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub").join("deeper")).unwrap();
        fs::write(dir.path().join("b.pdf"), "x").unwrap();
        fs::write(dir.path().join("a.png"), "x").unwrap();
        fs::write(dir.path().join("notes.txt"), "x").unwrap();
        fs::write(dir.path().join("sub").join("c.jpg"), "x").unwrap();
        fs::write(dir.path().join("sub").join("deeper").join("d.tiff"), "x").unwrap();

        let found = collect(&[dir.path().to_string_lossy().into_owned()]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.png", "b.pdf", "c.jpg", "d.tiff"]);
    }

    #[test]
    fn single_unsupported_files_and_missing_paths_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.txt");
        let pdf = dir.path().join("a.pdf");
        fs::write(&text, "x").unwrap();
        fs::write(&pdf, "x").unwrap();
        let found = collect(&[
            text.to_string_lossy().into_owned(),
            dir.path()
                .join("missing.pdf")
                .to_string_lossy()
                .into_owned(),
            pdf.to_string_lossy().into_owned(),
        ]);
        assert_eq!(found, vec![pdf]);
    }

    #[test]
    fn free_names_are_used_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_outputs(dir.path(), "invoice", &BOTH);
        assert_eq!(plan.docx, Some(dir.path().join("invoice.docx")));
        assert_eq!(
            plan.markdown,
            Some((
                dir.path().join("invoice.md"),
                dir.path().join("invoice_assets")
            ))
        );
    }

    #[test]
    fn a_taken_name_gets_the_next_number() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("invoice.docx"), "x").unwrap();
        let plan = plan_outputs(dir.path(), "invoice", &[ExportFormat::Docx]);
        assert_eq!(plan.docx, Some(dir.path().join("invoice (2).docx")));

        fs::write(dir.path().join("invoice (2).docx"), "x").unwrap();
        let plan = plan_outputs(dir.path(), "invoice", &[ExportFormat::Docx]);
        assert_eq!(plan.docx, Some(dir.path().join("invoice (3).docx")));
    }

    #[test]
    fn formats_of_one_conversion_share_the_number() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("invoice.docx"), "x").unwrap();
        let plan = plan_outputs(dir.path(), "invoice", &BOTH);
        assert_eq!(plan.docx, Some(dir.path().join("invoice (2).docx")));
        assert_eq!(plan.markdown.unwrap().0, dir.path().join("invoice (2).md"));
    }

    #[test]
    fn an_existing_assets_folder_counts_as_taken() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("invoice_assets")).unwrap();
        let plan = plan_outputs(dir.path(), "invoice", &[ExportFormat::Markdown]);
        assert_eq!(
            plan.markdown,
            Some((
                dir.path().join("invoice (2).md"),
                dir.path().join("invoice (2)_assets")
            ))
        );
    }

    #[test]
    fn only_requested_formats_are_planned() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_outputs(dir.path(), "x", &[ExportFormat::Markdown]);
        assert!(plan.docx.is_none());
        assert!(plan.markdown.is_some());
    }
}
