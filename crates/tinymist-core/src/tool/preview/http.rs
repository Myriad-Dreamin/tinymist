//! Document preview tool for Typst

use std::net::SocketAddr;
use std::sync::LazyLock;

use hyper::header::HeaderValue;
use hyper::service::service_fn;
use hyper_tungstenite::HyperWebsocket;
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use lsp_types::Url;
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{mpsc, oneshot};

/// created by `make_http_server`
pub struct HttpServer {
    /// The address the server is listening on.
    pub addr: SocketAddr,
    /// The sender to shutdown the server.
    pub shutdown_tx: oneshot::Sender<()>,
    /// The join handle of the server.
    pub join: tokio::task::JoinHandle<()>,
}

/// Create a http server for the previewer.
pub async fn make_http_server(
    frontend_html: String,
    static_file_addr: String,
    websocket_tx: mpsc::UnboundedSender<HyperWebsocket>,
) -> HttpServer {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    type Server = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

    let listener = tokio::net::TcpListener::bind(&static_file_addr)
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    log::info!("preview server listening on http://{addr}");

    let frontend_html = hyper::body::Bytes::from(frontend_html);
    let make_service = move || {
        let frontend_html = frontend_html.clone();
        let websocket_tx = websocket_tx.clone();
        let static_file_addr = static_file_addr.clone();
        service_fn(move |mut req: hyper::Request<Incoming>| {
            let frontend_html = frontend_html.clone();
            let websocket_tx = websocket_tx.clone();
            let static_file_addr = static_file_addr.clone();
            async move {
                // When a user visits a website in a browser, that website can try to connect to
                // our http / websocket server on `127.0.0.1` which may leak sensitive
                // information. We could use CORS headers to explicitly disallow
                // this. However, for Websockets, this does not work. Thus, we
                // manually check the `Origin` header. Browsers always send this
                // header for cross-origin requests.
                //
                // Important: This does _not_ protect against malicious users that share the
                // same computer as us (i.e. multi- user systems where the users
                // don't trust each other). In this case, malicious attackers can _still_
                // connect to our http / websocket servers (using a browser and
                // otherwise). And additionally they can impersonate a tinymist
                // http / websocket server towards a legitimate frontend/html client.
                // This requires additional protection that may be added in the future.
                let origin_header = req.headers().get("Origin");
                if origin_header
                    .is_some_and(|h| !is_valid_origin(h, &static_file_addr, addr.port()))
                {
                    anyhow::bail!(
                        "Connection with unexpected `Origin` header. Closing connection."
                    );
                }

                // Check if the request is a websocket upgrade request.
                if hyper_tungstenite::is_upgrade_request(&req) {
                    if origin_header.is_none() {
                        log::error!("websocket connection is not set `Origin` header, which will be a hard error in the future.");
                    }

                    let Some((response, websocket)) = hyper_tungstenite::upgrade(&mut req, None)
                        .log_error("Error in websocket upgrade")
                    else {
                        anyhow::bail!("cannot upgrade as websocket connection");
                    };

                    let _ = websocket_tx.send(websocket);

                    // Return the response so the spawned future can continue.
                    Ok(response)
                } else if req.uri().path() == "/" {
                    // log::debug!("Serve frontend: {mode:?}");
                    let res = hyper::Response::builder()
                        .header(hyper::header::CONTENT_TYPE, "text/html")
                        .body(Full::<Bytes>::from(frontend_html))
                        .unwrap();
                    Ok(res)
                } else {
                    // jump to /
                    let res = hyper::Response::builder()
                        .status(hyper::StatusCode::FOUND)
                        .header(hyper::header::LOCATION, "/")
                        .body(Full::<Bytes>::default())
                        .unwrap();
                    Ok(res)
                }
            }
        })
    };

    let (shutdown_tx, rx) = tokio::sync::oneshot::channel();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel();

    // the graceful watcher
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();

    let serve_conn = move |server: &Server, graceful: &GracefulShutdown, conn| {
        let (stream, _peer_addr) = match conn {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("accept error: {e}");
                return;
            }
        };

        let conn = server.serve_connection_with_upgrades(TokioIo::new(stream), make_service());
        let conn = graceful.watch(conn.into_owned());
        tokio::spawn(async move {
            conn.await.log_error("cannot serve http");
        });
    };

    let join = tokio::spawn(async move {
        // when this signal completes, start shutdown
        let mut signal = std::pin::pin!(final_rx);

        let mut server = Server::new(hyper_util::rt::TokioExecutor::new());
        server.http1().keep_alive(true);

        loop {
            tokio::select! {
                conn = listener.accept() => serve_conn(&server, &graceful, conn),
                Ok(_) = &mut signal => {
                    log::info!("graceful shutdown signal received");
                    break;
                }
            }
        }

        tokio::select! {
            _ = graceful.shutdown() => {
                log::info!("Gracefully shutdown!");
            },
            _ = tokio::time::sleep(reflexo::time::Duration::from_secs(10)) => {
                log::info!("Waited 10 seconds for graceful shutdown, aborting...");
            }
        }
    });
    tokio::spawn(async move {
        let _ = rx.await;
        final_tx.send(()).ok();
        log::info!("Preview server joined");
    });

    HttpServer {
        addr,
        shutdown_tx,
        join,
    }
}

fn is_valid_origin(h: &HeaderValue, static_file_addr: &str, expected_port: u16) -> bool {
    static GITPOD_ID_AND_HOST: LazyLock<Option<(String, String)>> = LazyLock::new(|| {
        let workspace_id = std::env::var("GITPOD_WORKSPACE_ID").ok();
        let cluster_host = std::env::var("GITPOD_WORKSPACE_CLUSTER_HOST").ok();
        workspace_id.zip(cluster_host)
    });

    is_valid_origin_impl(h, static_file_addr, expected_port, &GITPOD_ID_AND_HOST)
}

// Separate function so we can do gitpod-related tests without relying on env
// vars.
fn is_valid_origin_impl(
    origin_header: &HeaderValue,
    static_file_addr: &str,
    expected_port: u16,
    gitpod_id_and_host: &Option<(String, String)>,
) -> bool {
    let Ok(Ok(origin_url)) = origin_header.to_str().map(Url::parse) else {
        return false;
    };

    // Path is not allowed in Origin headers
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Origin
    if origin_url.path() != "/" && origin_url.path() != "" {
        return false;
    };

    let expected_origin = {
        let expected_host = Url::parse(&format!("http://{static_file_addr}")).unwrap();
        let expected_host = expected_host.host_str().unwrap();
        // Don't take the port from `static_file_addr` (it may have a dummy port e.g.
        // `127.0.0.1:0`)
        format!("http://{expected_host}:{expected_port}")
    };

    let gitpod_expected_origin = gitpod_id_and_host
        .as_ref()
        .map(|(workspace_id, cluster_host)| {
            format!("https://{expected_port}-{workspace_id}.{cluster_host}")
        });

    *origin_header == expected_origin
        // tmistele (PR #1382): The VSCode webview panel needs an exception: It doesn't send `http://{static_file_addr}`
        // as `Origin`. Instead it sends `vscode-webview://<random>`. Thus, we allow any
        // `Origin` starting with `vscode-webview://` as well. I think that's okay from a security
        // point of view, because I think malicious websites can't trick browsers into sending
        // `vscode-webview://...` as `Origin`.
        || origin_url.scheme() == "vscode-webview"
        // `code-server` also needs an exception: It opens `http://localhost:8080/proxy/<port>` in
        // the browser and proxies requests through to tinymist (which runs at `127.0.0.1:<port>`).
        // Thus, the `Origin` header will be `http://localhost:8080` which doesn't match what
        // we expect. Thus, just always allow anything from localhost/127.0.0.1
        // https://github.com/Myriad-Dreamin/tinymist/issues/1350
        || (
            matches!(origin_url.host_str(), Some("localhost") | Some("127.0.0.1"))
            && origin_url.scheme() == "http"
        )
        // `gitpod` also needs an exception. It loads `https://<port>-<workspace>.<host>` in the browser
        // and proxies requests through to tinymist (which runs as `127.0.0.1:<port>`).
        // We can detect this by looking at the env variables (see `GITPOD_ID_AND_HOST` in `is_valid_origin(..)`)
        || gitpod_expected_origin.is_some_and(|o| o == *origin_header)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_origin(origin: &'static str, static_file_addr: &str, port: u16) -> bool {
        is_valid_origin(&HeaderValue::from_static(origin), static_file_addr, port)
    }

    #[test]
    fn test_valid_origin_localhost() {
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(check_origin("http://localhost:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://localhost:42", "127.0.0.1:0", 42));
        assert!(check_origin("http://localhost", "127.0.0.1:0", 42));

        assert!(check_origin("http://127.0.0.1:42", "localhost:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "localhost:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "localhost:0", 42));
        assert!(check_origin("http://localhost:42", "localhost:42", 42));
        assert!(check_origin("http://localhost:42", "localhost:0", 42));
        assert!(check_origin("http://localhost", "localhost:0", 42));
    }

    #[test]
    fn test_invalid_origin_localhost() {
        assert!(!check_origin("https://huh.io:8080", "127.0.0.1:42", 42));
        assert!(!check_origin("http://huh.io:8080", "127.0.0.1:42", 42));
        assert!(!check_origin("https://huh.io:443", "127.0.0.1:42", 42));
        assert!(!check_origin("http://huh.io:42", "127.0.0.1:0", 42));
        assert!(!check_origin("http://huh.io", "127.0.0.1:42", 42));
        assert!(!check_origin("https://huh.io", "127.0.0.1:42", 42));

        assert!(!check_origin("https://huh.io:8080", "localhost:42", 42));
        assert!(!check_origin("http://huh.io:8080", "localhost:42", 42));
        assert!(!check_origin("https://huh.io:443", "localhost:42", 42));
        assert!(!check_origin("http://huh.io:42", "localhost:0", 42));
        assert!(!check_origin("http://huh.io", "localhost:42", 42));
        assert!(!check_origin("https://huh.io", "localhost:42", 42));
    }

    #[test]
    fn test_invalid_origin_scheme() {
        assert!(!check_origin("ftp://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(!check_origin("ftp://localhost:42", "127.0.0.1:42", 42));
        assert!(!check_origin("ftp://127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(!check_origin("ftp://localhost:42", "127.0.0.1:0", 42));

        // The scheme must be specified.
        assert!(!check_origin("127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(!check_origin("localhost:42", "127.0.0.1:0", 42));
        assert!(!check_origin("localhost:42", "127.0.0.1:42", 42));
        assert!(!check_origin("127.0.0.1:42", "127.0.0.1:42", 42));
    }

    #[test]
    fn test_valid_origin_vscode() {
        assert!(check_origin("vscode-webview://it", "127.0.0.1:42", 42));
        assert!(check_origin("vscode-webview://it", "127.0.0.1:0", 42));
    }

    #[test]
    fn test_origin_manually_binding() {
        assert!(!check_origin("https://huh.io:8080", "huh.io:42", 42));
        assert!(!check_origin("http://huh.io:8080", "huh.io:42", 42));
        assert!(!check_origin("https://huh.io:443", "huh.io:42", 42));
        assert!(check_origin("http://huh.io:42", "huh.io:0", 42));
        assert!(!check_origin("http://huh.io", "huh.io:42", 42));
        assert!(!check_origin("https://huh.io", "huh.io:42", 42));

        assert!(check_origin("http://127.0.0.1:42", "huh.io:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "huh.io:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "huh.io:0", 42));
        assert!(check_origin("http://localhost:42", "huh.io:42", 42));
        assert!(check_origin("http://localhost:42", "huh.io:0", 42));

        assert!(!check_origin("https://huh2.io:8080", "huh.io:42", 42));
        assert!(!check_origin("http://huh2.io:8080", "huh.io:42", 42));
        assert!(!check_origin("https://huh2.io:443", "huh.io:42", 42));
        assert!(!check_origin("http://huh2.io:42", "huh.io:0", 42));
        assert!(!check_origin("http://huh2.io", "huh.io:42", 42));
        assert!(!check_origin("https://huh2.io", "huh.io:42", 42));
    }

    // https://github.com/Myriad-Dreamin/tinymist/issues/1350
    // the origin of code-server's proxy
    #[test]
    fn test_valid_origin_code_server_proxy() {
        assert!(check_origin(
            // The URL has path /proxy/45411 but that is not sent in the Origin header
            "http://localhost:8080",
            "127.0.0.1:42",
            42
        ));
        assert!(check_origin("http://localhost", "127.0.0.1:42", 42));
    }

    // the origin of gitpod
    #[test]
    fn test_valid_origin_gitpod_proxy() {
        fn check_gitpod_origin(
            origin: &'static str,
            static_file_addr: &str,
            port: u16,
            workspace: &str,
            cluster_host: &str,
        ) -> bool {
            is_valid_origin_impl(
                &HeaderValue::from_static(origin),
                static_file_addr,
                port,
                &Some((workspace.to_owned(), cluster_host.to_owned())),
            )
        }

        let check_gitpod_origin1 = |origin: &'static str| {
            let explicit =
                check_gitpod_origin(origin, "127.0.0.1:42", 42, "workspace_id", "gitpod.typ");
            let implicit =
                check_gitpod_origin(origin, "127.0.0.1:0", 42, "workspace_id", "gitpod.typ");

            assert_eq!(explicit, implicit, "failed port binding");
            explicit
        };

        assert!(check_gitpod_origin1("http://127.0.0.1:42"));
        assert!(check_gitpod_origin1("http://127.0.0.1:42"));
        assert!(check_gitpod_origin1("https://42-workspace_id.gitpod.typ"));
        assert!(!check_gitpod_origin1(
            // A path is not allowed in Origin header
            "https://42-workspace_id.gitpod.typ/path"
        ));
        assert!(!check_gitpod_origin1(
            // Gitpod always runs on default port
            "https://42-workspace_id.gitpod.typ:42"
        ));

        assert!(!check_gitpod_origin1("https://42-workspace_id2.gitpod.typ"));
        assert!(!check_gitpod_origin1("http://huh.io"));
        assert!(!check_gitpod_origin1("https://huh.io"));
    }
}
