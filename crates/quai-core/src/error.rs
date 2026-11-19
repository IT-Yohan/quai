use std::fmt;

/// Coarse error category, reported to the frontend as `ErrorReport::kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Usage,
    Permission,
    Network,
    NotFound,
    Conflict,
    Internal,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Usage => "usage",
            Kind::Permission => "permission",
            Kind::Network => "network",
            Kind::NotFound => "not_found",
            Kind::Conflict => "conflict",
            Kind::Internal => "internal",
        }
    }
}

/// An error with a user-facing message and a category.
///
/// Wrapped in `anyhow::Error` everywhere; the CLI downcasts to recover the kind.
#[derive(Debug)]
pub struct QuaiError {
    pub kind: Kind,
    pub message: String,
}

impl fmt::Display for QuaiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QuaiError {}

impl QuaiError {
    /// Build an `anyhow::Error` carrying a [`QuaiError`].
    pub fn wrap(kind: Kind, message: impl Into<String>) -> anyhow::Error {
        anyhow::Error::new(QuaiError { kind, message: message.into() })
    }
}

pub fn usage(message: impl Into<String>) -> anyhow::Error {
    QuaiError::wrap(Kind::Usage, message)
}

pub fn not_found(message: impl Into<String>) -> anyhow::Error {
    QuaiError::wrap(Kind::NotFound, message)
}

pub fn conflict(message: impl Into<String>) -> anyhow::Error {
    QuaiError::wrap(Kind::Conflict, message)
}

pub fn network(message: impl Into<String>) -> anyhow::Error {
    QuaiError::wrap(Kind::Network, message)
}

pub fn permission(message: impl Into<String>) -> anyhow::Error {
    QuaiError::wrap(Kind::Permission, message)
}

/// Category of any error, `Internal` unless it carries a [`QuaiError`].
pub fn kind_of(err: &anyhow::Error) -> Kind {
    for cause in err.chain() {
        if let Some(q) = cause.downcast_ref::<QuaiError>() {
            return q.kind;
        }
        if cause.downcast_ref::<reqwest::Error>().is_some() {
            return Kind::Network;
        }
    }
    Kind::Internal
}
