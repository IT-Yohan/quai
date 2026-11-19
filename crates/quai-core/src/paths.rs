use std::path::PathBuf;

/// Every location quai reads or writes.
///
/// Defaults are the system paths; each root can be overridden through the
/// environment, which the test-suite and development setups rely on:
/// `QUAI_CONFIG_DIR`, `QUAI_REPOS_DIR`, `QUAI_CACHE_DIR`, `QUAI_SYSTEMD_DIR`.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub repos_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub systemd_dir: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Paths {
            config_dir: PathBuf::from("/etc/quai"),
            repos_dir: PathBuf::from("/etc/yum.repos.d"),
            cache_dir: PathBuf::from("/var/cache/quai"),
            systemd_dir: PathBuf::from("/etc/systemd/system"),
        }
    }
}

impl Paths {
    pub fn from_env() -> Self {
        let mut p = Paths::default();
        if let Some(v) = std::env::var_os("QUAI_CONFIG_DIR") {
            p.config_dir = v.into();
        }
        if let Some(v) = std::env::var_os("QUAI_REPOS_DIR") {
            p.repos_dir = v.into();
        }
        if let Some(v) = std::env::var_os("QUAI_CACHE_DIR") {
            p.cache_dir = v.into();
        }
        if let Some(v) = std::env::var_os("QUAI_SYSTEMD_DIR") {
            p.systemd_dir = v.into();
        }
        p
    }

    pub fn global_config(&self) -> PathBuf {
        self.config_dir.join("quai.toml")
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.config_dir.join("sources.d")
    }

    pub fn source_file(&self, id: &str) -> PathBuf {
        self.sources_dir().join(format!("{id}.toml"))
    }

    pub fn private_dir(&self) -> PathBuf {
        self.config_dir.join("private")
    }

    pub fn tokens_file(&self) -> PathBuf {
        self.private_dir().join("tokens.toml")
    }

    pub fn secret_key(&self) -> PathBuf {
        self.private_dir().join("repo-key.sec.asc")
    }

    pub fn public_key(&self) -> PathBuf {
        self.config_dir.join("repo-key.asc")
    }

    pub fn repo_file(&self, id: &str) -> PathBuf {
        self.repos_dir.join(format!("quai-{id}.repo"))
    }

    pub fn headers_dir(&self) -> PathBuf {
        self.cache_dir.join("headers")
    }

    pub fn forge_cache(&self, id: &str) -> PathBuf {
        self.cache_dir.join("forge").join(format!("{id}.json"))
    }

    pub fn repo_cache_dir(&self, id: &str) -> PathBuf {
        self.cache_dir.join("repos").join(id)
    }

    pub fn repodata_dir(&self, id: &str) -> PathBuf {
        self.repo_cache_dir(id).join("repodata")
    }

    pub fn state_file(&self, id: &str) -> PathBuf {
        self.repo_cache_dir(id).join("state.json")
    }

    pub fn socket_dropin(&self) -> PathBuf {
        self.systemd_dir.join("quaid.socket.d").join("50-quai-listen.conf")
    }
}
