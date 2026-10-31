//! The layer that checks a router's requests (feature `axum`).

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};

use crate::{KeyInfo, KeyVerifier, Verdict};

/// The key a request was let in with; in the request's extensions.
#[derive(Clone, Debug)]
pub struct Authenticated(pub KeyInfo);

/// `router`, letting in only requests with `Authorization: Bearer <token>` for an
/// unretired key. Anything else is 401 with `WWW-Authenticate: Bearer`.
pub fn require_key(verifier: Arc<KeyVerifier>, router: Router) -> Router {
    router.layer(from_fn_with_state(verifier, check))
}

async fn check(
    State(verifier): State<Arc<KeyVerifier>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let Some(token) = token else {
        return unauthorized();
    };
    // The store may be read from disk.
    let verdict = tokio::task::spawn_blocking(move || verifier.verify(&token)).await;
    match verdict {
        Ok(Verdict::Accepted(info)) => {
            request.extensions_mut().insert(Authenticated(info));
            next.run(request).await
        }
        Ok(Verdict::Refused) => unauthorized(),
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
    use super::*;
    use crate::KeyStore;
    use axum::Extension;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt as _;

    fn app(dir: &std::path::Path) -> Router {
        let router = Router::new().route(
            "/",
            get(|Extension(who): Extension<Authenticated>| async move { who.0.name }),
        );
        require_key(Arc::new(KeyVerifier::new(dir)), router)
    }

    async fn call(app: Router, auth: Option<&str>) -> (StatusCode, Response) {
        let mut req = Request::builder().uri("/");
        if let Some(a) = auth {
            req = req.header(header::AUTHORIZATION, a);
        }
        let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        (res.status(), res)
    }

    fn assert_challenge(res: &Response) {
        assert_eq!(res.headers()[header::WWW_AUTHENTICATE], "Bearer");
    }

    #[tokio::test]
    async fn no_header_is_401() {
        let dir = tempfile::tempdir().unwrap();
        let (status, res) = call(app(dir.path()), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_challenge(&res);
    }

    #[tokio::test]
    async fn malformed_header_is_401() {
        let dir = tempfile::tempdir().unwrap();
        for h in ["Basic x", "Bearer ", "Bearer", "bearer"] {
            let (status, res) = call(app(dir.path()), Some(h)).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{h:?}");
            assert_challenge(&res);
        }
    }

    #[tokio::test]
    async fn wrong_token_is_401() {
        let dir = tempfile::tempdir().unwrap();
        KeyStore::open(dir.path())
            .unwrap()
            .add("p", None, None)
            .unwrap();
        let (status, _) = call(app(dir.path()), Some("Bearer nope")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn good_token_reaches_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (_, token) = KeyStore::open(dir.path())
            .unwrap()
            .add("pendant", None, None)
            .unwrap();
        let (status, res) =
            call(app(dir.path()), Some(&format!("Bearer {}", token.expose()))).await;
        assert_eq!(status, StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"pendant");
    }
}
