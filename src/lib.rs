//! This crate allows [`hyper`](https://docs.rs/hyper) servers to accept websocket connections, backed by [`tungstenite`](https://docs.rs/tungstenite).
//!
//! The [`upgrade`] function allows you to upgrade a HTTP connection to a websocket connection.
//! It returns a HTTP response to send to the client, and a future that resolves to a [`WebSocketStream`].
//! The response must be sent to the client for the future to be resolved.
//! In practise this means that you must spawn the future in a different task.
//!
//! Note that the [`upgrade`] function itself does not check if the request is actually an upgrade request.
//! For simple cases, you can check this using the [`is_upgrade_request`] function before calling [`upgrade`].
//! For more complicated cases where the server should support multiple upgrade protocols,
//! you can manually inspect the `Connection` and `Upgrade` headers.
//!
//! # Example
//! ```no_run
//! use futures::{sink::SinkExt, stream::StreamExt};
//! use hyper::{Body, Request, Response};
//! use hyper_tungstenite::{tungstenite, HyperWebsocket};
//! use tungstenite::Message;
//! # fn foo(message: &Message) {}
//!
//! /// Handle a HTTP or WebSocket request.
//! async fn handle_request(request: Request<Body>) -> Result<Response<Body>, Box<dyn std::error::Error>> {
//!     // Check if the request is a websocket upgrade request.
//!     if hyper_tungstenite::is_upgrade_request(&request) {
//!         let (response, websocket) = hyper_tungstenite::upgrade(request, None)?;
//!
//!         // Spawn a task to handle the websocket connection.
//!         tokio::spawn(async move {
//!             if let Err(e) = serve_websocket(websocket).await {
//!                 eprintln!("Error in websocket connection: {}", e);
//!             }
//!         });
//!
//!         // Return the response so the spawned future can continue.
//!         Ok(response)
//!     } else {
//!         // Handle regular HTTP requests here.
//!         Ok(Response::new(Body::from("Hello HTTP!")))
//!     }
//! }
//!
//! /// Handle a websocket connection.
//! async fn serve_websocket(websocket: HyperWebsocket) -> Result<(), Box<dyn std::error::Error>> {
//!     let mut websocket = websocket.await?;
//!     while let Some(message) = websocket.next().await {
//!         let message = message?;
//!
//!         // Do something with the message.
//!         foo(&message);
//!
//!         // Send a reply.
//!         websocket.send(Message::text("Thank you, come again.")).await?;
//!     }
//!
//!     Ok(())
//! }
//! ```

use hyper::{Body, Request, Response};
use std::task::{Context, Poll};
use std::pin::Pin;
use pin_project::pin_project;

use tungstenite::{Error, error::ProtocolError};
use tungstenite::protocol::{Role, WebSocketConfig};

pub use tokio_tungstenite::tungstenite;
pub use tokio_tungstenite::WebSocketStream;
pub use hyper;

/// A future that resolves to a websocket stream when the associated HTTP upgrade completes.
#[pin_project]
#[derive(Debug)]
pub struct HyperWebsocket {
	#[pin]
	inner: hyper::upgrade::OnUpgrade,
	config: Option<WebSocketConfig>,
}

/// Try to upgrade a received `hyper::Request` to a websocket connection.
///
/// The function returns a HTTP response and a future that resolves to the websocket stream.
/// The response body *MUST* be sent to the client before the future can be resolved.
///
/// This functions checks `Sec-WebSocket-Key` and `Sec-WebSocket-Version` headers.
/// It does not inspect the `Origin`, `Sec-WebSocket-Protocol` or `Sec-WebSocket-Extensions` headers.
/// You can inspect the headers manually before calling this function,
/// and modify the response headers appropriately.
///
/// This function also does not look at the `Connection` or `Upgrade` headers.
/// To check if a request is a websocket upgrade request, you can use [`is_upgrade_request`].
/// Alternatively you can inspect the `Connection` and `Upgrade` headers manually.
///
pub fn upgrade(
	request: Request<Body>,
	config: Option<WebSocketConfig>,
) -> Result<(Response<Body>, HyperWebsocket), ProtocolError> {
	let key = request.headers().get("Sec-WebSocket-Key")
		.ok_or(ProtocolError::MissingSecWebSocketKey)?;
	if request.headers().get("Sec-WebSocket-Version").map(|v| v.as_bytes()) != Some(b"13") {
		return Err(ProtocolError::MissingSecWebSocketVersionHeader);
	}

	let response = Response::builder()
		.status(hyper::StatusCode::SWITCHING_PROTOCOLS)
		.header(hyper::header::CONNECTION, "upgrade")
		.header(hyper::header::UPGRADE, "websocket")
		.header("Sec-WebSocket-Accept", &convert_key(key.as_bytes()))
		.body(Body::from("switching to websocket protocol"))
		.expect("bug: failed to build response");

	let stream = HyperWebsocket {
		inner: hyper::upgrade::on(request),
		config,
	};

	Ok((response, stream))
}

/// Check if a request is a websocket upgrade request.
///
/// If the `Upgrade` header lists multiple protocols,
/// this function returns true if of them are `"websocket"`,
/// If the server supports multiple upgrade protocols,
/// it would be more appropriate to try each listed protocol in order.
pub fn is_upgrade_request<B>(request: &hyper::Request<B>) -> bool {
	header_contains_value(request.headers(), hyper::header::CONNECTION, "Upgrade")
		&& header_contains_value(request.headers(), hyper::header::UPGRADE, "websocket")
}

/// Check if there is a header of the given name containing the wanted value.
fn header_contains_value(headers: &hyper::HeaderMap, header: impl hyper::header::AsHeaderName, value: impl AsRef<[u8]>) -> bool {
	let value = value.as_ref();
	for header in headers.get_all(header) {
		if header.as_bytes().split(|&c| c == b',').any(|x| trim(x).eq_ignore_ascii_case(value)) {
			return true;
		}
	}
	false
}

fn trim(data: &[u8]) -> &[u8] {
	trim_end(trim_start(data))
}

fn trim_start(data: &[u8]) -> &[u8] {
	if let Some(start) =data.iter().position(|x| !x.is_ascii_whitespace()) {
		&data[start..]
	} else {
		b""
	}
}

fn trim_end(data: &[u8]) -> &[u8] {
	if let Some(last) = data.iter().rposition(|x| !x.is_ascii_whitespace()) {
		&data[..last + 1]
	} else {
		b""
	}
}

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept.
fn convert_key(input: &[u8]) -> String {
	use sha1::Digest;

	// ... field is constructed by concatenating /key/ ...
	// ... with the string "258EAFA5-E914-47DA-95CA-C5AB0DC85B11" (RFC 6455)
	const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
	let mut sha1 = sha1::Sha1::default();
	sha1.update(input);
	sha1.update(WS_GUID);
	base64::encode(sha1.finalize())
}

impl std::future::Future for HyperWebsocket {
	type Output = Result<WebSocketStream<hyper::upgrade::Upgraded>, Error>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		let this = self.project();
		let upgraded = match this.inner.poll(cx) {
			Poll::Pending => return Poll::Pending,
			Poll::Ready(x) => x,
		};

		let upgraded = upgraded.map_err(|_| Error::Protocol(ProtocolError::HandshakeIncomplete))?;

		let stream = WebSocketStream::from_raw_socket(
			upgraded,
			Role::Server,
			this.config.take(),
		);
		tokio::pin!(stream);

		// The future returned by `from_raw_socket` is always ready.
		// Not sure why it is a future in the first place.
		match stream.as_mut().poll(cx) {
			Poll::Pending => unreachable!("from_raw_socket should always be created ready"),
			Poll::Ready(x) => Poll::Ready(Ok(x)),
		}
	}
}
