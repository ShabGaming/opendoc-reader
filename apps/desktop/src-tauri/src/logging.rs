//! File logging with rotation, and the log excerpt used by the diagnostics text.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

const LOG_PREFIX: &str = "opendoc-reader";
const LOG_SUFFIX: &str = "log";
const KEEP_FILES: usize = 7;

/// Starts logging to `<logs>/opendoc-reader.<date>.log`, one file per day, the last seven kept.
/// Hold the returned guard for the life of the app: dropping it flushes and stops the writer.
pub fn init(logs_dir: &Path) -> Option<WorkerGuard> {
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(KEEP_FILES)
        .build(logs_dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("OPENDOC_LOG")
        .unwrap_or_else(|_| EnvFilter::new("opendoc_reader_lib=info,engine=info,llama=info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false);
    let result = if cfg!(debug_assertions) {
        builder.with_writer(writer.and(std::io::stderr)).try_init()
    } else {
        builder.with_writer(writer).try_init()
    };
    result.ok().map(|()| guard)
}

/// The newest log file, if any.
fn latest_log(logs_dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(logs_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(LOG_PREFIX) && n.ends_with(LOG_SUFFIX))
        })
        .collect();
    files.sort();
    files.pop()
}

/// The last `count` lines of the newest log file, with the given strings (the home folder, the
/// app data folder) replaced so the text can be pasted into an issue.
pub fn recent_lines(logs_dir: &Path, count: usize, redact: &[(String, &str)]) -> Vec<String> {
    let Some(file) = latest_log(logs_dir) else {
        return Vec::new();
    };
    let Ok(handle) = std::fs::File::open(file) else {
        return Vec::new();
    };
    let lines: Vec<String> = BufReader::new(handle)
        .lines()
        .map_while(std::result::Result::ok)
        .collect();
    let start = lines.len().saturating_sub(count);
    lines[start..]
        .iter()
        .map(|line| redact_line(line, redact))
        .collect()
}

pub fn redact_line(line: &str, redact: &[(String, &str)]) -> String {
    let mut out = line.to_string();
    for (needle, replacement) in redact {
        if !needle.is_empty() {
            out = out.replace(needle.as_str(), replacement);
            // Windows paths show up with doubled backslashes inside debug-formatted strings.
            out = out.replace(&needle.replace('\\', "\\\\"), replacement);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_replaces_every_listed_path() {
        let line = r"opened C:\Users\Ada\scan.pdf and C:\\Users\\Ada\\scan.pdf";
        let out = redact_line(line, &[(r"C:\Users\Ada".to_string(), "<home>")]);
        assert_eq!(out, r"opened <home>\scan.pdf and <home>\\scan.pdf");
    }

    #[test]
    fn recent_lines_reads_the_newest_file_and_redacts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("opendoc-reader.2026-10-14.log"), "old\n").unwrap();
        std::fs::write(
            dir.path().join("opendoc-reader.2026-10-15.log"),
            "one\ntwo /home/ada/x\nthree\n",
        )
        .unwrap();
        let lines = recent_lines(dir.path(), 2, &[("/home/ada".to_string(), "<home>")]);
        assert_eq!(lines, vec!["two <home>/x".to_string(), "three".to_string()]);
    }

    #[test]
    fn no_log_folder_gives_no_lines() {
        assert!(recent_lines(Path::new("/definitely/not/here"), 5, &[]).is_empty());
    }
}
