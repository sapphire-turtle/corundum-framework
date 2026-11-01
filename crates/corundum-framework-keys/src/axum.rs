//! The layer that checks a router's requests (feature `axum`).

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};

use crate::{KeyInfo, KeyVerifier, Verdict};

/// The key a request was let in with, and its resolved profile; in the request's
/// extensions.
#[derive(Clone, Debug)]
pub struct Authenticated<P> {
    /// The key.
    pub key: KeyInfo,
    /// What `resolve` made of the key's profile.
    pub profile: P,
}

/// `router`, letting in only requests with `Authorization: Bearer <token>` for an
/// unretired key whose profile `resolve` knows.
///
/// - No token, or one that matches no unretired key: 401 with `WWW-Authenticate: Bearer`.
/// - `resolve` returns `None` (a profile the app no longer has): 403.
/// - Otherwise the handler runs, with [`Authenticated`] in the request's extensions.
///
/// `resolve` runs on a blocking thread, so it may read files or a database; it is called on
/// every request, so a profile removed is refused at once. What each profile may do is the
/// app's to check, in its handlers or its own middleware.
pub fn require_key<P, F>(verifier: Arc<KeyVerifier>, resolve: F, router: Router) -> Router
where
    P: Clone + Send + Sync + 'static,
    F: Fn(&KeyInfo) -> Option<P> + Send + Sync + 'static,
{
    let gate = Arc::new(Gate { verifier, resolve });
    router.layer(from_fn_with_state(gate, check::<P, F>))
}

struct Gate<F> {
    verifier: Arc<KeyVerifier>,
    resolve: F,
}

enum Outcome<P> {
    Refused,
    Unresolved(KeyInfo),
    Resolved(Authenticated<P>),
}

async fn check<P, F>(State(gate): State<Arc<Gate<F>>>, mut request: Request, next: Next) -> Response
where
    P: Clone + Send + Sync + 'static,
    F: Fn(&KeyInfo) -> Option<P> + Send + Sync + 'static,
{
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        // The scheme is case-insensitive (RFC 7235).
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let Some(token) = token else {
        return unauthorized();
    };
    // The store may be read from disk, and `resolve` may read the app's own.
    let outcome = tokio::task::spawn_blocking(move || match gate.verifier.verify(&token) {
        Verdict::Refused => Outcome::Refused,
        Verdict::Accepted(key) => match (gate.resolve)(&key) {
            Some(profile) => Outcome::Resolved(Authenticated { key, profile }),
            None => Outcome::Unresolved(key),
        },
    })
    .await;
    match outcome {
        Ok(Outcome::Resolved(who)) => {
            request.extensions_mut().insert(who);
            next.run(request).await
        }
        Ok(Outcome::Unresolved(key)) => {
            tracing::warn!(key = %key.id, profile = %key.profile, "the key's profile is not known; refused");
            StatusCode::FORBIDDEN.into_response()
        }
        Ok(Outcome::Refused) => unauthorized(),
        Err(err) => {
            tracing::error!("the key check did not finish: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use super::*;
    use crate::{KeyStore, ProfileId};
    use axum::Extension;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt as _;

    type Known = Arc<Mutex<HashSet<String>>>;

    fn known(ids: &[&str]) -> Known {
        Arc::new(Mutex::new(ids.iter().map(|s| s.to_string()).collect()))
    }

    fn app_with<F>(dir: &std::path::Path, resolve: F) -> Router
    where
        F: Fn(&KeyInfo) -> Option<String> + Send + Sync + 'static,
    {
        let router = Router::new().route(
            "/",
            get(
                |Extension(who): Extension<Authenticated<String>>| async move {
                    format!("{} {}", who.key.name, who.profile)
                },
            ),
        );
        require_key(Arc::new(KeyVerifier::new(dir)), resolve, router)
    }

    fn app(dir: &std::path::Path, known: Known) -> Router {
        app_with(dir, move |k| {
            let id = k.profile.as_str();
            known.lock().unwrap().contains(id).then(|| id.to_owned())
        })
    }

    fn add(dir: &std::path::Path, name: &str, profile: &str) -> crate::Token {
        KeyStore::open(dir)
            .unwrap()
            .add(name, None, ProfileId::new(profile).unwrap())
            .unwrap()
            .1
    }

    async fn call(app: Router, auth: Option<&str>) -> (StatusCode, Response) {
        let mut req = Request::builder().uri("/");
        if let Some(a) = auth {
            req = req.header(header::AUTHORIZATION, a);
        }
        let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        (res.status(), res)
    }

    fn bearer(token: &crate::Token) -> String {
        format!("Bearer {}", token.expose())
    }

    fn assert_challenge(res: &Response) {
        assert_eq!(res.headers()[header::WWW_AUTHENTICATE], "Bearer");
    }

    #[tokio::test]
    async fn no_header_is_401() {
        let dir = tempfile::tempdir().unwrap();
        let (status, res) = call(app(dir.path(), known(&["admin"])), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_challenge(&res);
    }

    #[tokio::test]
    async fn malformed_header_is_401() {
        let dir = tempfile::tempdir().unwrap();
        for h in ["Basic x", "Bearer ", "Bearer", "bearer"] {
            let (status, res) = call(app(dir.path(), known(&["admin"])), Some(h)).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{h:?}");
            assert_challenge(&res);
        }
    }

    #[tokio::test]
    async fn wrong_token_is_401() {
        let dir = tempfile::tempdir().unwrap();
        add(dir.path(), "p", "admin");
        let (status, _) = call(app(dir.path(), known(&["admin"])), Some("Bearer nope")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn good_token_reaches_handler() {
        let dir = tempfile::tempdir().unwrap();
        let token = add(dir.path(), "pendant", "admin");
        let (status, res) = call(app(dir.path(), known(&["admin"])), Some(&bearer(&token))).await;
        assert_eq!(status, StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"pendant admin");
    }

    #[tokio::test]
    async fn scheme_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let token = add(dir.path(), "p", "admin");
        for scheme in ["bearer", "BEARER"] {
            let (status, _) = call(
                app(dir.path(), known(&["admin"])),
                Some(&format!("{scheme} {}", token.expose())),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{scheme}");
        }
    }

    #[tokio::test]
    async fn unresolved_profile_is_403() {
        let dir = tempfile::tempdir().unwrap();
        let token = add(dir.path(), "p", "gone");
        let (status, res) = call(app(dir.path(), known(&["admin"])), Some(&bearer(&token))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(res.headers().get(header::WWW_AUTHENTICATE).is_none());
    }

    #[tokio::test]
    async fn resolver_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let token = add(dir.path(), "p", "admin");
        let set = known(&["admin"]);
        let router = app(dir.path(), set.clone());
        let (status, _) = call(router.clone(), Some(&bearer(&token))).await;
        assert_eq!(status, StatusCode::OK);
        set.lock().unwrap().remove("admin");
        let (status, _) = call(router, Some(&bearer(&token))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn panicking_resolver_is_500() {
        let dir = tempfile::tempdir().unwrap();
        let token = add(dir.path(), "p", "admin");
        let router = app_with(dir.path(), |_| -> Option<String> { panic!("resolver") });
        let (status, _) = call(router, Some(&bearer(&token))).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
