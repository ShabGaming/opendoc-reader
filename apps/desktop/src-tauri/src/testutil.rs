//! Test helpers shared by the unit tests: a small HTTP server with `Range` support and the
//! failure modes the downloader has to survive.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiny_http::{Header, Response, Server, StatusCode};

pub struct Route {
    body: Vec<u8>,
    status: Option<u16>,
    fail_first: u32,
    cut_first_at: Option<usize>,
    ignore_range: bool,
    delay_per_chunk: Option<Duration>,
}

impl Route {
    pub fn new(body: Vec<u8>) -> Self {
        Self {
            body,
            status: None,
            fail_first: 0,
            cut_first_at: None,
            ignore_range: false,
            delay_per_chunk: None,
        }
    }

    /// Always answers with this status and no body.
    pub fn status(code: u16) -> Self {
        Self {
            status: Some(code),
            ..Self::new(Vec::new())
        }
    }

    /// Answers the first `n` requests with 500.
    pub fn fail_first_requests(mut self, n: u32) -> Self {
        self.fail_first = n;
        self
    }

    /// Closes the connection after `bytes` bytes of the first response.
    pub fn cut_first_response_at(mut self, bytes: usize) -> Self {
        self.cut_first_at = Some(bytes);
        self
    }

    /// Answers a range request with the whole body and status 200.
    pub fn ignore_range(mut self) -> Self {
        self.ignore_range = true;
        self
    }

    /// Sleeps between 16 KiB chunks of the body.
    pub fn slow(mut self, per_chunk: Duration) -> Self {
        self.delay_per_chunk = Some(per_chunk);
        self
    }
}

struct Entry {
    route: Route,
    hits: AtomicU32,
    ranges: Mutex<Vec<Option<String>>>,
}

pub struct TestServer {
    base: String,
    entries: Arc<HashMap<String, Entry>>,
}

struct Body {
    data: Vec<u8>,
    pos: usize,
    cut_at: Option<usize>,
    delay: Option<Duration>,
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(cut) = self.cut_at {
            if self.pos >= cut {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "cut",
                ));
            }
        }
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        let mut end = (self.pos + buf.len().min(16 * 1024)).min(self.data.len());
        if let Some(cut) = self.cut_at {
            end = end.min(cut);
        }
        let n = end - self.pos;
        buf[..n].copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(n)
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn parse_range_start(value: &str) -> Option<usize> {
    value
        .strip_prefix("bytes=")?
        .split('-')
        .next()?
        .parse()
        .ok()
}

impl TestServer {
    pub fn start(routes: Vec<(&str, Route)>) -> Self {
        let server = Server::http("127.0.0.1:0").expect("bind test server");
        let base = format!(
            "http://{}",
            server.server_addr().to_ip().expect("ip address")
        );
        let entries: Arc<HashMap<String, Entry>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, route)| {
                    (
                        path.to_string(),
                        Entry {
                            route,
                            hits: AtomicU32::new(0),
                            ranges: Mutex::new(Vec::new()),
                        },
                    )
                })
                .collect(),
        );
        let shared = entries.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let entries = shared.clone();
                std::thread::spawn(move || Self::handle(&entries, request));
            }
        });
        Self { base, entries }
    }

    fn handle(entries: &HashMap<String, Entry>, request: tiny_http::Request) {
        let path = request.url().to_string();
        let Some(entry) = entries.get(&path) else {
            let _ = request.respond(Response::empty(StatusCode(404)));
            return;
        };
        let nth = entry.hits.fetch_add(1, Ordering::SeqCst);
        let range = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Range"))
            .map(|h| h.value.as_str().to_string());
        entry.ranges.lock().unwrap().push(range.clone());

        let route = &entry.route;
        if let Some(code) = route.status {
            let _ = request.respond(Response::empty(StatusCode(code)));
            return;
        }
        if nth < route.fail_first {
            let _ = request.respond(Response::empty(StatusCode(500)));
            return;
        }
        let total = route.body.len();
        let start = if route.ignore_range {
            None
        } else {
            range.as_deref().and_then(parse_range_start)
        };
        let (status, offset) = match start {
            Some(s) if s < total => (206, s),
            Some(_) => {
                let _ = request.respond(Response::empty(StatusCode(416)));
                return;
            }
            None => (200, 0),
        };
        let data = route.body[offset..].to_vec();
        let cut_at = if nth == 0 { route.cut_first_at } else { None };
        let len = data.len();
        let reader = Body {
            data,
            pos: 0,
            cut_at,
            delay: route.delay_per_chunk,
        };
        let mut headers = vec![header("Accept-Ranges", "bytes")];
        if status == 206 {
            headers.push(header(
                "Content-Range",
                &format!("bytes {offset}-{}/{total}", total - 1),
            ));
        }
        let response = Response::new(StatusCode(status), headers, reader, Some(len), None);
        let _ = request.respond(response);
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn hits(&self, path: &str) -> u32 {
        self.entries[path].hits.load(Ordering::SeqCst)
    }

    /// The `Range` header of each request to `path`, in order.
    pub fn range_headers(&self, path: &str) -> Vec<Option<String>> {
        self.entries[path].ranges.lock().unwrap().clone()
    }

    /// A URL on a port nothing listens on, so connecting fails at once.
    pub fn dead_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}/nothing")
    }
}
