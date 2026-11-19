use anyhow::{Context, Result};
use quai_core::config::{self, GlobalConfig, Tokens};
use quai_core::engine::Engine;
use quai_core::error::{network, permission, usage};
use quai_core::paths::Paths;
use quai_core::system;
use quai_types::RefreshReply;
use std::io::{BufRead, IsTerminal, Write};
use std::time::Duration;

pub struct Ctx {
    pub paths: Paths,
    pub global: GlobalConfig,
    pub json: bool,
    pub yes: bool,
}

impl Ctx {
    pub fn new(json: bool, yes: bool) -> Result<Ctx> {
        let paths = Paths::from_env();
        let global = config::load_global(&paths)?;
        Ok(Ctx { paths, global, json, yes })
    }

    /// Commands that write under /etc need root. `QUAI_SKIP_ROOT_CHECK=1` is for
    /// development trees where the paths point somewhere writable.
    pub fn require_root(&self, what: &str) -> Result<()> {
        if system::is_root() || std::env::var_os("QUAI_SKIP_ROOT_CHECK").is_some() {
            return Ok(());
        }
        Err(permission(format!("{what} changes system configuration: run it with sudo")))
    }

    /// Tokens from the protected file when readable, plus the usual environment variables.
    pub fn tokens(&self) -> Tokens {
        let mut t = config::load_tokens(&self.paths);
        for (var, host) in [("GITHUB_TOKEN", "github.com"), ("GH_TOKEN", "github.com"), ("GITLAB_TOKEN", "gitlab.com")] {
            if let Ok(v) = std::env::var(var) {
                if !v.trim().is_empty() {
                    t.tokens.entry(host.to_string()).or_insert_with(|| v.trim().to_string());
                }
            }
        }
        t
    }

    /// Engine for one-off forge queries; it reads quaid's cache but never writes it.
    pub fn engine(&self) -> Result<Engine> {
        Engine::new(self.paths.clone(), self.global.clone(), self.tokens(), None, false)
    }

    pub fn daemon_url(&self, path: &str) -> String {
        format!("http://{}{}", self.global.daemon.listen, path)
    }

    fn local_client(timeout: Duration) -> Result<reqwest::Client> {
        // Never route 127.0.0.1 through a corporate proxy.
        Ok(reqwest::Client::builder().no_proxy().timeout(timeout).build()?)
    }

    pub async fn daemon_healthy(&self) -> bool {
        let Ok(c) = Self::local_client(Duration::from_secs(3)) else { return false };
        match c.get(self.daemon_url("/-/health")).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    /// Ask quaid to re-check forges now. `None` = every source.
    pub async fn daemon_refresh(&self, id: Option<&str>) -> Result<RefreshReply> {
        let c = Self::local_client(Duration::from_secs(600))?;
        let path = match id {
            Some(id) => format!("/-/refresh/{id}"),
            None => "/-/refresh".to_string(),
        };
        let resp = c.post(self.daemon_url(&path)).send().await.map_err(|e| {
            network(format!(
                "quaid is not reachable on {} ({e}); run `sudo quai setup` or `systemctl status quaid.socket`",
                self.global.daemon.listen
            ))
        })?;
        if !resp.status().is_success() {
            return Err(network(format!("quaid answered HTTP {}", resp.status())));
        }
        resp.json().await.context("decoding quaid's answer")
    }

    pub fn interactive(&self) -> bool {
        !self.json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    }

    /// Ask for confirmation, unless `--yes`. Non-interactive runs must pass `--yes`.
    pub fn confirm(&self, question: &str) -> Result<()> {
        if self.yes {
            return Ok(());
        }
        if !self.interactive() {
            return Err(usage(format!("{question} Pass --yes to confirm non-interactively.")));
        }
        eprint!("{question} [y/N] ");
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes" | "o" | "oui") {
            Ok(())
        } else {
            Err(usage("cancelled"))
        }
    }

    /// Progress messages go to stderr so `--json` stdout stays clean.
    pub fn note(&self, msg: impl AsRef<str>) {
        if !self.json {
            eprintln!("{}", msg.as_ref());
        }
    }
}
