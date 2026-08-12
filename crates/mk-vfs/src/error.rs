//! Typed backend errors carrying an errno-equivalent and the resolved remote
//! path, so `EACCES · /etc/nginx/nginx.conf not writable by deploy` is
//! reconstructible end to end (plan.md E4-S1).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsErrorKind {
    NotFound,
    PermissionDenied,
    Io,
    Timeout,
    Unreachable,
    Protocol,
    ReadOnly,
    HostKeyUnknown,
    HostKeyChanged,
    Cancelled,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VfsError {
    pub kind: VfsErrorKind,
    /// POSIX errno equivalent when known (`13` = EACCES).
    pub errno: Option<i32>,
    /// The resolved remote path the operation failed on.
    pub path: Option<String>,
    pub message: String,
}

impl VfsError {
    pub fn new(kind: VfsErrorKind, message: impl Into<String>) -> Self {
        VfsError {
            kind,
            errno: None,
            path: None,
            message: message.into(),
        }
    }

    pub fn with_errno(mut self, errno: i32) -> Self {
        self.errno = Some(errno);
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.errno, &self.path) {
            (Some(e), Some(p)) => write!(f, "{e} · {p} · {}", self.message),
            (Some(e), None) => write!(f, "{e} · {}", self.message),
            (None, Some(p)) => write!(f, "{p} · {}", self.message),
            (None, None) => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for VfsError {}
