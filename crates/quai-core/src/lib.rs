//! quai-core: everything quai does, independent of how it is invoked.
//!
//! * [`spec`] turns `owner/repo` or a forge URL into a [`forge::ForgeRef`].
//! * [`forge`] lists releases on GitHub, GitLab and Forgejo.
//! * [`rpm`] reads RPM headers over HTTP range requests.
//! * [`repodata`] writes `repomd.xml`, `primary`, `filelists` and `other`.
//! * [`sign`] signs `repomd.xml` with the machine-local OpenPGP key.
//! * [`engine`] ties it together and caches results on disk.
//! * [`config`], [`repofile`] and [`paths`] manage the files under `/etc`.

pub mod config;
pub mod engine;
pub mod error;
pub mod evr;
pub mod forge;
pub mod fsutil;
pub mod http;
pub mod paths;
pub mod repodata;
pub mod repofile;
pub mod rpm;
pub mod sign;
pub mod spec;
pub mod system;

pub use error::{Kind, QuaiError};
pub use quai_types as types;

/// Version of quai, shared by every binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
