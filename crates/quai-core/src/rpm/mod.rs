pub mod fetch;
pub mod header;

pub use fetch::{fetch_header, sha256_of_url, FetchedHeader};
pub use header::{parse_file_bytes, parse_package, Dep, FileEntry, FileKind, Layout, PackageInfo};
