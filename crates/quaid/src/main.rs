//! quaid — serves one rpm-md repository per quai source on 127.0.0.1.
//!
//! Started by `quaid.socket` on the first connection, exits after a period of
//! inactivity. Routes:
//!
//! * `GET  /repos/<id>/repodata/<file>`        metadata (refreshed on `repomd.xml`)
//! * `GET  /repos/<id>/packages/<key>/<file>`  302 to the forge's download URL
//! * `POST /-/refresh[/<id>]`                  re-check forges now (rate-limited)
//! * `GET  /-/health`                          liveness

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use quai_core::config::{self, SourceConfig};
use quai_core::engine::{Engine, HeaderMeta};
use quai_core::paths::Paths;
use quai_core::sign::Signer;
use quai_types::RefreshReply;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Minimum delay between two forced refreshes of the same source
/// (protects the forge's API budget). `QUAID_FORCE_COOLDOWN_SECS` overrides it.
fn force_cooldown() -> Duration {
    let secs = std::env::var("QUAID_FORCE_COOLDOWN_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
    Duration::from_secs(secs)
}

struct AppState {
    engine: Engine,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    forced: Mutex<HashMap<String, Instant>>,
    last_activity: AtomicI64,
    in_flight: AtomicUsize,
}

impl AppState {
    fn lock_for(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.locks.lock().unwrap();
        map.entry(id.to_string()).or_default().clone()
    }

    fn touch(&self) {
        self.last_activity.store(now_secs(), Ordering::Relaxed);
    }

    /// May `id` be force-refreshed now? Records the attempt.
    fn may_force(&self, id: &str) -> bool {
        let mut map = self.forced.lock().unwrap();
        let now = Instant::now();
        match map.get(id) {
            Some(t) if now.duration_since(*t) < force_cooldown() => false,
            _ => {
                map.insert(id.to_string(), now);
                true
            }
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

struct Args {
    listen: Option<String>,
    idle_timeout: Option<u64>,
}

fn parse_args() -> Result<Args> {
    let mut args = Args { listen: None, idle_timeout: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--listen" => args.listen = Some(it.next().context("--listen needs an address")?),
            "--idle-timeout" => {
                args.idle_timeout = Some(it.next().context("--idle-timeout needs seconds")?.parse()?);
            }
            "--version" => {
                println!("quaid {}", quai_core::VERSION);
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!(
                    "quaid {}: quai's local repository gateway\n\n\
                     Usually started by quaid.socket. Options:\n  \
                     --listen ADDR         bind ADDR instead of using the systemd socket\n  \
                     --idle-timeout SECS   exit after SECS idle seconds (0 = never)\n\n\
                     Environment: QUAI_CONFIG_DIR, QUAI_CACHE_DIR, QUAID_LOG (e.g. debug)",
                    quai_core::VERSION
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    Ok(args)
}

/// The listening socket handed over by systemd (`LISTEN_FDS`), if any.
fn systemd_listener() -> Result<Option<std::net::TcpListener>> {
    let pid_ok = std::env::var("LISTEN_PID").ok().and_then(|p| p.parse::<u32>().ok()) == Some(std::process::id());
    let fds = std::env::var("LISTEN_FDS").ok().and_then(|n| n.parse::<i32>().ok()).unwrap_or(0);
    if !pid_ok || fds < 1 {
        return Ok(None);
    }
    std::env::remove_var("LISTEN_PID");
    std::env::remove_var("LISTEN_FDS");
    std::env::remove_var("LISTEN_FDNAMES");
    use std::os::fd::FromRawFd;
    // SAFETY: systemd passes the first socket as fd 3 and nothing else owns it.
    let listener = unsafe { std::net::TcpListener::from_raw_fd(3) };
    listener.set_nonblocking(true)?;
    Ok(Some(listener))
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_env("QUAID_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let under_systemd = std::env::var_os("INVOCATION_ID").is_some();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(!under_systemd)
        .without_time()
        .init();

    let args = parse_args()?;
    let paths = Paths::from_env();
    let global = config::load_global(&paths)?;
    let tokens = config::load_tokens(&paths);
    let signer = if global.defaults.sign_metadata {
        match Signer::load(&paths.secret_key()) {
            Ok(s) => {
                info!("signing metadata with key {}", s.fingerprint());
                Some(s)
            }
            Err(e) => {
                warn!("metadata will be unsigned and dnf will refuse it: {e:#}. Run `quai setup`.");
                None
            }
        }
    } else {
        None
    };

    let (listener, activated) = match (systemd_listener()?, &args.listen) {
        (Some(l), None) => (tokio::net::TcpListener::from_std(l)?, true),
        (_, listen) => {
            let addr = listen.clone().unwrap_or_else(|| global.daemon.listen.clone());
            let l = tokio::net::TcpListener::bind(&addr).await.with_context(|| format!("binding {addr}"))?;
            (l, false)
        }
    };
    let idle = args.idle_timeout.unwrap_or(if activated { global.daemon.idle_timeout_secs } else { 0 });
    info!(
        "quaid {} listening on {}{}",
        quai_core::VERSION,
        listener.local_addr()?,
        if activated { " (socket activation)" } else { "" }
    );

    let state = Arc::new(AppState {
        engine: Engine::new(paths, global, tokens, signer, true)?,
        locks: Mutex::new(HashMap::new()),
        forced: Mutex::new(HashMap::new()),
        last_activity: AtomicI64::new(now_secs()),
        in_flight: AtomicUsize::new(0),
    });

    let app = Router::new()
        .route("/repos/{id}/repodata/{file}", get(repodata))
        .route("/repos/{id}/packages/{key}/{file}", get(package))
        .route("/-/health", get(|| async { "ok\n" }))
        .route("/-/refresh", post(refresh_all))
        .route("/-/refresh/{id}", post(refresh_one))
        .layer(middleware::from_fn_with_state(state.clone(), track_activity))
        .with_state(state.clone());

    axum::serve(listener, app).with_graceful_shutdown(shutdown(state, idle)).await?;
    info!("quaid stopped");
    Ok(())
}

async fn track_activity(State(st): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    st.in_flight.fetch_add(1, Ordering::SeqCst);
    st.touch();
    let resp = next.run(req).await;
    st.touch();
    st.in_flight.fetch_sub(1, Ordering::SeqCst);
    resp
}

/// Resolve on SIGTERM/SIGINT, or once idle for `idle` seconds (0 = never).
async fn shutdown(st: Arc<AppState>, idle: u64) {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler");
    let idle_watch = async {
        if idle == 0 {
            std::future::pending::<()>().await;
        }
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let quiet = now_secs() - st.last_activity.load(Ordering::Relaxed);
            if st.in_flight.load(Ordering::SeqCst) == 0 && quiet >= idle as i64 {
                info!("idle for {quiet}s, exiting (systemd restarts quaid on demand)");
                return;
            }
        }
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
        _ = idle_watch => {}
    }
}

fn error(status: StatusCode, msg: impl Into<String>) -> Response {
    let mut body = msg.into();
    body.push('\n');
    (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

fn enabled_source(st: &AppState, id: &str) -> Result<SourceConfig, Box<Response>> {
    match config::load_source(&st.engine.paths, id) {
        Ok(s) if s.enabled => Ok(s),
        Ok(_) => Err(Box::new(error(StatusCode::NOT_FOUND, format!("source '{id}' is disabled")))),
        Err(e) => Err(Box::new(error(StatusCode::NOT_FOUND, format!("{e:#}")))),
    }
}

async fn repodata(State(st): State<Arc<AppState>>, Path((id, file)): Path<(String, String)>) -> Response {
    let src = match enabled_source(&st, &id) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let path = match st.engine.repodata_path(&id, &file) {
        Ok(p) => p,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid file name"),
    };
    // One refresh at a time per source; repomd.xml and its signature are read under the same lock.
    let lock = st.lock_for(&id);
    let _guard = lock.lock().await;
    if file == "repomd.xml" {
        if let Err(e) = st.engine.refresh(&src, false).await {
            warn!("{id}: {e:#}");
            return error(StatusCode::BAD_GATEWAY, format!("quai could not build metadata for '{id}': {e:#}"));
        }
    }
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let ctype = if file.ends_with(".xml") {
                "application/xml"
            } else if file.ends_with(".gz") {
                "application/gzip"
            } else if file.ends_with(".asc") {
                "application/pgp-signature"
            } else {
                "application/octet-stream"
            };
            let mut resp = Response::new(Body::from(bytes));
            resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
            resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            resp
        }
        Err(_) => error(StatusCode::NOT_FOUND, format!("{file} not found for '{id}'")),
    }
}

/// Packages are never stored: redirect dnf to the forge, it verifies the SHA-256 itself.
async fn package(State(st): State<Arc<AppState>>, Path((id, key, file)): Path<(String, String, String)>) -> Response {
    if let Err(r) = enabled_source(&st, &id) {
        return *r;
    }
    let url = Engine::load_snapshot(&st.engine.paths, &id)
        .and_then(|snap| snap.packages.into_iter().find(|p| p.asset_key == key && p.file_name == file).map(|p| p.url))
        .or_else(|| {
            // Metadata cached by dnf may predate the current snapshot: the header cache still knows the URL.
            if !key.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let meta = std::fs::read(st.engine.paths.headers_dir().join(format!("{key}.json"))).ok()?;
            serde_json::from_slice::<HeaderMeta>(&meta).ok().map(|m| m.url)
        });
    match url {
        Some(url) => match HeaderValue::from_str(&url) {
            Ok(loc) => {
                let mut resp = StatusCode::FOUND.into_response();
                resp.headers_mut().insert(header::LOCATION, loc);
                resp
            }
            Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "invalid upstream URL"),
        },
        None => error(StatusCode::NOT_FOUND, format!("unknown package {file}; run `dnf makecache --refresh`")),
    }
}

async fn refresh_ids(st: &AppState, ids: Vec<String>) -> RefreshReply {
    let mut reply = RefreshReply { refreshed: vec![], skipped: vec![], errors: vec![] };
    for id in ids {
        let src = match config::load_source(&st.engine.paths, &id) {
            Ok(s) if s.enabled => s,
            Ok(_) => {
                reply.skipped.push(id);
                continue;
            }
            Err(e) => {
                reply.errors.push((id, format!("{e:#}")));
                continue;
            }
        };
        if !st.may_force(&id) {
            reply.skipped.push(id);
            continue;
        }
        let lock = st.lock_for(&id);
        let _guard = lock.lock().await;
        match st.engine.refresh(&src, true).await {
            Ok(snap) => match snap.last_error {
                Some(err) => reply.errors.push((id, err)),
                None => reply.refreshed.push(id),
            },
            Err(e) => reply.errors.push((id, format!("{e:#}"))),
        }
    }
    reply
}

async fn refresh_all(State(st): State<Arc<AppState>>) -> Json<RefreshReply> {
    let (sources, _) = config::load_sources(&st.engine.paths);
    let ids = sources.into_iter().map(|s| s.id).collect();
    Json(refresh_ids(&st, ids).await)
}

async fn refresh_one(State(st): State<Arc<AppState>>, Path(id): Path<String>) -> Json<RefreshReply> {
    Json(refresh_ids(&st, vec![id]).await)
}
