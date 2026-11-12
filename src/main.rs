mod auth;
mod config;
mod procs;
mod spa;

use std::sync::Arc;

use axum::{
    Router,
    http::header,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{
    auth::Auth,
    config::{ManagedProcConfig, load_configs},
    procs::Procs,
};
use tracing_subscriber::EnvFilter;

struct AppState {
    auth: Auth,   // credentials + active sessions
    procs: Procs, // managed child processes
    configs: RwLock<Vec<ManagedProcConfig>>,
}
type Shared = Arc<AppState>;

/// The VT323 pixel font, compiled into the binary (SIL Open Font License,
/// see assets/VT323-OFL.txt). Public, because the login page needs it too.
async fn font() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        &include_bytes!("../assets/vt323.woff2")[..],
    )
}

fn hash_password_cmd() {
    use argon2::{Argon2, password_hash::PasswordHasher};

    eprint!("Password: ");
    let mut pw = String::new();

    std::io::stdin()
        .read_line(&mut pw)
        .expect("failed to read password");
    let pw = pw.trim_end_matches(['\r', '\n']);

    let hash = Argon2::default()
        .hash_password(pw.as_bytes())
        .expect("hashing failed");
    println!("{hash}");
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("hash-password") {
        return hash_password_cmd();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weber=debug,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    info!("Load configurations...");
    let configs = load_configs().await.unwrap();
    info!("{} records loaded.", configs.len());

    let state = Arc::new(AppState {
        auth: Auth::from_env(),
        procs: Procs::new(),
        configs: RwLock::new(configs),
    });

    let protected = Router::new()
        .route("/api/status", get(procs::status))
        .route("/api/start/{name}", post(procs::start))
        .route("/api/stop/{name}", post(procs::stop))
        .route("/api/logs/{name}", get(procs::logs))
        .route("/api/me", get(auth::me))
        .route("/api/processes", post(procs::add))
        .route("/api/processes/{name}", delete(procs::delete))
        .route("/api/processes/{name}", put(procs::update_with_restart))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_login,
        ));

    let app = Router::new()
        .route("/login", get(auth::login_page).post(auth::login_submit))
        .route("/logout", post(auth::logout))
        .route("/assets/vt323.woff2", get(font))
        .merge(protected)
        .fallback(spa::static_handler) //everything else: Svelte app
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("Server started: {}", addr);

    axum::serve(listener, app).await.unwrap();
}
