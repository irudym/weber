use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use askama::Template;
use axum::{
    Extension, Form,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::Shared;

const COOKIE_NAME: &str = "procmon_session";
const SESSION_IDLE_TTL: Duration = Duration::from_secs(8 * 60 * 60); // 8 hours

pub struct Auth {
    user: String,
    pass_hash: String,
    secure_cookie: bool,
    sessions: Mutex<HashMap<String, Session>>, // token -> session
}

struct Session {
    user: String,
    expires: Instant,
}

#[derive(Clone)]
pub struct CurrentUser(pub String);

impl Auth {
    pub fn from_env() -> Self {
        let user = std::env::var("MONITOR_USER").expect("Env MONITOR_USER is not set");
        let pass_hash =
            std::env::var("MONITOR_PASS_HASH").expect("Env MONITOR_PASS_HASH is not set");

        // check if the hash is malformed
        PasswordHash::new(&pass_hash).expect("MONITOR_PASS_HASH is not valid argon2 hash");

        let secure_cookie = std::env::var("MONITOR_INSECURE_COOKIE").is_err();

        Self {
            user,
            pass_hash,
            secure_cookie,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn verify(&self, user: &str, pass: &str) -> bool {
        let Ok(hash) = PasswordHash::new(&self.pass_hash) else {
            return false;
        };

        //Run the expensive check first, even if the username is wrong, to not reveal which usernames exist.
        let pass_ok = Argon2::default()
            .verify_password(pass.as_bytes(), &hash)
            .is_ok();

        pass_ok && user == self.user
    }

    async fn create_session(&self, user: String) -> String {
        let token = random_token();
        let session = Session {
            user,
            expires: Instant::now() + SESSION_IDLE_TTL,
        };
        self.sessions.lock().await.insert(token.clone(), session);
        token
    }

    async fn validate(&self, token: &str) -> Option<String> {
        let now = Instant::now();
        let mut sessions = self.sessions.lock().await;

        // throw away expiered sessions
        sessions.retain(|_, s| s.expires > now);

        let s = sessions.get_mut(token)?;
        s.expires = now + SESSION_IDLE_TTL;
        Some(s.user.clone())
    }

    async fn destroy(&self, token: &str) {
        self.sessions.lock().await.remove(token);
    }

    fn session_cookie(&self, token: String) -> Cookie<'static> {
        Cookie::build((COOKIE_NAME, token))
            .path("/")
            .http_only(true) //JS cannot read it
            .secure(self.secure_cookie) //for http should be false
            .same_site(SameSite::Strict)
            .build()
    }
}

pub async fn require_login(
    State(st): State<Shared>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    // 1. read the cookie and look up the session
    let user = match jar.get(COOKIE_NAME) {
        Some(c) => st.auth.validate(c.value()).await,
        None => None,
    };

    match user {
        // 2a. Valid: attach the username to request and continue
        Some(user) => {
            req.extensions_mut().insert(CurrentUser(user));
            next.run(req).await
        }
        // 2b. Invalid, API call: 401, so JS can handle it
        None if req.uri().path().starts_with("/api/") => StatusCode::UNAUTHORIZED.into_response(),
        // 2c. Invalid, page load: redirect to login form
        None => Redirect::to("/login").into_response(),
    }
}

fn random_token() -> String {
    let mut buf = [0u8; 32]; // 256 bits: impossible to guess
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .expect("cannot read /dev/urandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

// Login Handlers
#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage {
    error: String,
}

fn render_login(error: &str) -> Response {
    match (LoginPage {
        error: error.to_owned(),
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn login_page(State(st): State<Shared>, jar: CookieJar) -> Response {
    // valid session / login goes straight to the dashboard
    if let Some(c) = jar.get(COOKIE_NAME) {
        if st.auth.validate(c.value()).await.is_some() {
            return Redirect::to("/").into_response();
        }
    }
    render_login("")
}

// Processing the form
#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login_submit(
    State(st): State<Shared>,
    jar: CookieJar,
    Form(form): Form<LoginForm>, // parse application/x-www-form-urlencoded
) -> Response {
    // Argon2 takes ten of millisecs of pure CPU
    let st2 = st.clone();
    let user = form.username.clone();
    let ok = tokio::task::spawn_blocking(move || st2.auth.verify(&user, &form.password))
        .await
        .unwrap_or(false);

    if !ok {
        // make each wrong guess cost the attacker 1 second
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut resp = render_login("Invalid username or password.");
        *resp.status_mut() = StatusCode::UNAUTHORIZED;
        return resp;
    }

    let token = st.auth.create_session(form.username).await;
    let jar = jar.add(st.auth.session_cookie(token));

    (jar, Redirect::to("/")).into_response()
}

pub async fn logout(State(st): State<Shared>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(COOKIE_NAME) {
        st.auth.destroy(c.value()).await;
    }
    let jar = jar.remove(Cookie::build(COOKIE_NAME).path("/"));
    (jar, Redirect::to("/login")).into_response()
}

/// Lets handlers behind the middleware read the logged-in user.
pub type User = Extension<CurrentUser>;
