//! The gateway's request bounds, against a mocked HTTP server on loopback:
//! each test injects a 1 KiB cap and a short timeout, so nothing waits
//! longer than that timeout.

use std::sync::Arc;
use std::time::Duration;

use crisp_core::sign::SecretKeySet;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use crate::config::ServerConfig;
use crate::gateway::{Gateway, HttpGateway, Limits, SearchQuery};
use crate::signer::Signer;

const LIMITS: Limits = Limits {
    timeout: Duration::from_millis(300),
    max_response_bytes: 1024,
};

/// How the mocked server answers a request whose path starts with a route.
#[derive(Clone, Copy)]
enum Reply {
    /// 200 with this body and content type, its length declared.
    Body(&'static [u8], &'static str),
    /// 200, chunked (no declared length), with this many bytes.
    Chunked(usize),
    /// 200 declaring this length, then nothing.
    Declares(u64),
    /// 200 declaring 100 bytes, sending 10, then nothing.
    StallsMidBody,
    /// Nothing at all.
    Stalls,
}

/// Serves `routes` on a loopback port; returns the base URL.
async fn serve(routes: &'static [(&'static str, Reply)]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(answer(stream, routes));
        }
    });
    url
}

/// Reads one request off `stream` and answers it per `routes`.
async fn answer(mut stream: TcpStream, routes: &'static [(&'static str, Reply)]) {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    let head_end = loop {
        let read = stream.read(&mut buffer).await.expect("reads");
        assert!(read > 0, "the request ended before its head");
        request.extend_from_slice(&buffer[..read]);
        if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&request[..head_end]).to_ascii_lowercase();
    let declared: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .map_or(0, |length| length.trim().parse().expect("a length"));
    while request.len() < head_end + declared {
        let read = stream.read(&mut buffer).await.expect("reads");
        request.extend_from_slice(&buffer[..read]);
    }
    let path = head.split_whitespace().nth(1).expect("a path").to_owned();
    let reply = routes
        .iter()
        .find(|(route, _)| path.starts_with(route))
        .map_or_else(|| panic!("no route for {path}"), |(_, reply)| *reply);
    let ok = "HTTP/1.1 200 OK\r\nconnection: close\r\n";
    let written = match reply {
        Reply::Body(body, kind) => {
            let head = format!(
                "{ok}content-type: {kind}\r\ncontent-length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(&[head.as_bytes(), body].concat()).await
        }
        Reply::Chunked(total) => {
            let mut out = format!("{ok}transfer-encoding: chunked\r\n\r\n").into_bytes();
            for chunk in [256; 64].iter().scan(total, |left, size| {
                let take = (*left).min(*size);
                *left -= take;
                (take > 0).then_some(take)
            }) {
                out.extend_from_slice(format!("{chunk:x}\r\n").as_bytes());
                out.extend(std::iter::repeat_n(b'x', chunk));
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(b"0\r\n\r\n");
            stream.write_all(&out).await
        }
        Reply::Declares(length) => {
            let head = format!("{ok}content-length: {length}\r\n\r\n");
            stream.write_all(head.as_bytes()).await
        }
        Reply::StallsMidBody => {
            let head = format!("{ok}content-length: 100\r\n\r\n0123456789");
            stream.write_all(head.as_bytes()).await
        }
        Reply::Stalls => Ok(()),
    };
    written.expect("writes");
    if matches!(
        reply,
        Reply::Declares(_) | Reply::StallsMidBody | Reply::Stalls
    ) {
        // Hold the connection open, sending nothing more.
        std::future::pending::<()>().await;
    }
}

fn gateway(url: String, signer: Signer) -> HttpGateway {
    let config = ServerConfig {
        url,
        username: "cook".into(),
    };
    HttpGateway::with_limits(config, Arc::new(signer), LIMITS).expect("gateway")
}

async fn download(routes: &'static [(&'static str, Reply)]) -> Result<Option<Vec<u8>>, String> {
    let gateway = gateway(serve(routes).await, Signer::new(None));
    gateway.download("rcp1x").await.map_err(|e| e.0)
}

const BUNDLE: &str = "/api/v1/bundles/";
const BINARY: &str = "application/octet-stream";

#[test]
fn the_default_limits_are_the_documented_constants() {
    let limits = Limits::default();
    assert_eq!(limits.timeout, Duration::from_secs(120));
    assert_eq!(
        limits.max_response_bytes,
        crisp_core::limits::MAX_BUNDLE_SIZE as u64
    );
}

#[tokio::test]
async fn a_response_within_the_cap_downloads() {
    static ROUTES: [(&str, Reply); 1] = [(BUNDLE, Reply::Body(b"bundle", BINARY))];
    assert_eq!(download(&ROUTES).await, Ok(Some(b"bundle".to_vec())));
    static EXACT: [(&str, Reply); 1] = [(BUNDLE, Reply::Chunked(1024))];
    assert_eq!(download(&EXACT).await, Ok(Some(vec![b'x'; 1024])));
}

#[tokio::test]
async fn a_declared_oversized_response_is_refused_unread() {
    static ROUTES: [(&str, Reply); 1] = [(BUNDLE, Reply::Declares(4096))];
    let error = download(&ROUTES).await.expect_err("refused");
    assert!(
        error.starts_with("downloading: ")
            && error.contains("1024-byte limit")
            && error.contains("declared 4096 bytes"),
        "{error}"
    );
}

#[tokio::test]
async fn an_undeclared_oversized_response_is_cut_off_at_the_cap() {
    static ROUTES: [(&str, Reply); 1] = [(BUNDLE, Reply::Chunked(1025))];
    let error = download(&ROUTES).await.expect_err("refused");
    assert!(
        error.starts_with("downloading: ")
            && error.contains("1024-byte limit")
            && error.contains("cut off after"),
        "{error}"
    );
}

#[tokio::test]
async fn a_stalled_response_times_out_before_or_during_its_body() {
    static HEAD: [(&str, Reply); 1] = [(BUNDLE, Reply::Stalls)];
    static BODY: [(&str, Reply); 1] = [(BUNDLE, Reply::StallsMidBody)];
    for routes in [&HEAD, &BODY] {
        let error = download(routes).await.expect_err("times out");
        assert!(
            error.starts_with("downloading: ") && error.contains("300ms request timeout"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn search_is_bounded_too() {
    static OK: [(&str, Reply); 1] = [(
        "/api/v1/search",
        Reply::Body(
            br#"{"limit":20,"offset":0,"recipes":[]}"#,
            "application/json",
        ),
    )];
    let found = gateway(serve(&OK).await, Signer::new(None))
        .search(SearchQuery::default())
        .await
        .expect("searches");
    assert!(found.is_empty());
    static BIG: [(&str, Reply); 1] = [("/api/v1/search", Reply::Chunked(4096))];
    let error = gateway(serve(&BIG).await, Signer::new(None))
        .search(SearchQuery::default())
        .await
        .expect_err("refused");
    assert!(
        error.0.starts_with("searching: ") && error.0.contains("1024-byte limit"),
        "{error}"
    );
}

const CHALLENGE: (&str, Reply) = (
    "/api/v1/auth/challenge",
    Reply::Body(
        br#"{"audience":"usr1a","nonce":"00ff"}"#,
        "application/json",
    ),
);
const LOGIN: (&str, Reply) = (
    "/api/v1/auth/login",
    Reply::Body(
        br#"{"expires_at":0,"token":"t","usr_id":"usr1a"}"#,
        "application/json",
    ),
);

fn signer() -> Signer {
    Signer::from_secret(SecretKeySet::from_seed(&[7u8; 32]))
}

#[tokio::test]
async fn a_stalled_sign_in_times_out_without_a_registration_hint() {
    static ROUTES: [(&str, Reply); 1] = [("/api/v1/auth/challenge", Reply::Stalls)];
    let error = gateway(serve(&ROUTES).await, signer())
        .publish(vec![1])
        .await
        .expect_err("times out");
    assert!(
        error.0.starts_with("starting sign-in: ")
            && error.0.contains("request timeout")
            && !error.0.contains("registered"),
        "{error}"
    );
}

#[tokio::test]
async fn publish_is_bounded_after_signing_in() {
    static BIG: [(&str, Reply); 3] = [CHALLENGE, LOGIN, ("/api/v1/bundles", Reply::Declares(4096))];
    let error = gateway(serve(&BIG).await, signer())
        .publish(vec![1])
        .await
        .expect_err("refused");
    assert!(
        error.0.starts_with("publishing: ") && error.0.contains("declared 4096 bytes"),
        "{error}"
    );
    static SLOW: [(&str, Reply); 3] = [CHALLENGE, LOGIN, ("/api/v1/bundles", Reply::Stalls)];
    let error = gateway(serve(&SLOW).await, signer())
        .publish(vec![1])
        .await
        .expect_err("times out");
    assert!(
        error.0.starts_with("publishing: ") && error.0.contains("request timeout"),
        "{error}"
    );
    static OK: [(&str, Reply); 3] = [
        CHALLENGE,
        LOGIN,
        ("/api/v1/bundles", Reply::Body(b"endorsed", BINARY)),
    ];
    let endorsed = gateway(serve(&OK).await, signer())
        .publish(vec![1])
        .await
        .expect("publishes");
    assert_eq!(endorsed, b"endorsed");
}
