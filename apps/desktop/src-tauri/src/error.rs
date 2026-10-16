//! Helpers around the contract's [`AppError`].

use crate::contract::{AppError, AppErrorCode};

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            reason: None,
            detail: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::InvalidArgument, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::NotFound, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Internal, message)
    }

    pub fn cancelled() -> Self {
        Self::new(AppErrorCode::Cancelled, "Cancelled.").with_reason("cancelled")
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)?;
        if let Some(reason) = &self.reason {
            write!(f, " ({reason})")?;
        }
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

/// True for the OS errors that mean the disk is full (`ENOSPC`, `ERROR_DISK_FULL`,
/// `ERROR_HANDLE_DISK_FULL`).
pub fn is_disk_full(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::StorageFull
        || matches!(error.raw_os_error(), Some(28) | Some(112) | Some(39))
}

fn io_reason(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::AlreadyExists => "already_exists",
        _ => "write_failed",
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        if is_disk_full(&error) {
            AppError::new(AppErrorCode::DiskFull, "The disk is full.")
                .with_reason("disk_full")
                .with_detail(error.to_string())
        } else {
            AppError::new(AppErrorCode::Io, "A file operation failed.")
                .with_reason(io_reason(error.kind()))
                .with_detail(error.to_string())
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::internal("Malformed JSON.").with_detail(error.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::internal("The queue database failed.").with_detail(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_full_io_errors_map_to_disk_full() {
        let error = std::io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 });
        let app: AppError = error.into();
        assert_eq!(app.code, AppErrorCode::DiskFull);
        assert_eq!(app.reason.as_deref(), Some("disk_full"));
    }

    #[test]
    fn other_io_errors_map_to_io() {
        let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let app: AppError = error.into();
        assert_eq!(app.code, AppErrorCode::Io);
        assert_eq!(app.reason.as_deref(), Some("permission_denied"));
    }
}
