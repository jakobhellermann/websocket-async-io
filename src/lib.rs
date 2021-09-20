//! Implementations of [`AsyncRead`](https://docs.rs/futures/0.3.17/futures/io/trait.AsyncRead.html) and [`AsyncWrite`](https://docs.rs/futures/0.3.17/futures/io/trait.AsyncWrite.html) on top of websockets using [`web-sys`](https://github.com/rustwasm/wasm-bindgen/tree/master/crates/web-sys))
//! # Example
//! ```rust,no_run
//! # async fn run() -> Result<(), std::io::Error> {
//! let ws = WebsocketIO::new("localhost:8000").await?;
//! let (mut reader, mut writer) = ws.split();
//!
//! writer.write_all(&[0, 1, 2, 3, 93]).await?;
//! writer.write_all(&[42, 34, 93]).await?;
//! writer.write_all(&[0, 0, 1, 2, 93]).await?;
//!
//! let mut buf = Vec::new();
//! for _ in 0..3 {
//!     reader.read_until(93, &mut buf).await?;
//!     console_log!("{:?}", buf);
//!     buf.clear();
//! }
//!
//! # Ok(())
//! # }
/// ```
use std::cmp::Ordering;
use std::pin::Pin;
use std::task::Poll;

use futures_channel::mpsc::Receiver;
use futures_core::stream::Stream;
use futures_io::AsyncBufRead;
use futures_io::AsyncRead;
use futures_io::AsyncWrite;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

pub struct WebsocketIO {
    ws: WebSocket,
    reader: WebsocketReader,
}

struct WebsocketReader {
    read_rx: Receiver<Uint8Array>,
    remaining: Vec<u8>,
}
struct WebsocketWriter {
    ws: WebSocket,
}

impl WebsocketIO {
    pub async fn new(addr: &str) -> Result<WebsocketIO, std::io::Error> {
        WebsocketIO::new_inner(&format!("ws://{}", addr)).await
    }
    pub async fn new_wss(addr: &str) -> Result<WebsocketIO, std::io::Error> {
        WebsocketIO::new_inner(&format!("wss://{}", addr)).await
    }

    async fn new_inner(url: &str) -> Result<WebsocketIO, std::io::Error> {
        let ws =
            WebSocket::new(url).map_err(|e| -> std::io::Error { todo!("map error: {:?}", e) })?;

        let buffer = 4;

        let (open_tx, open_rx) = futures_channel::oneshot::channel();
        let (read_tx, read_rx) = futures_channel::mpsc::channel(buffer);

        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            let mut read_tx = read_tx.clone();
            let blob = match e.data().dyn_into::<web_sys::Blob>() {
                Ok(blob) => blob,
                _ => return,
            };

            let fr = web_sys::FileReader::new().unwrap();
            let fr_c = fr.clone();
            let file_reader_load_end = Closure::wrap(Box::new(move |_e: web_sys::ProgressEvent| {
                let array = Uint8Array::new(&fr_c.result().unwrap());
                read_tx.start_send(array).unwrap();
            })
                as Box<dyn FnMut(web_sys::ProgressEvent)>);
            fr.set_onloadend(Some(file_reader_load_end.as_ref().unchecked_ref()));
            file_reader_load_end.forget();

            fr.read_as_array_buffer(&blob).expect("blob not readable");
        }) as Box<dyn Fn(MessageEvent)>);

        let onerror_callback =
            Closure::wrap(Box::new(move |_: ErrorEvent| {}) as Box<dyn FnMut(ErrorEvent)>);

        let mut open_tx = Some(open_tx);
        let onopen_callback =
            Closure::wrap(Box::new(move |_| open_tx.take().unwrap().send(()).unwrap())
                as Box<dyn FnMut(JsValue)>);

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let reader = WebsocketReader {
            read_rx,
            remaining: Vec::new(),
        };

        open_rx.await.unwrap();

        let ws_io = WebsocketIO { ws, reader };
        Ok(ws_io)
    }

    pub fn split(self) -> (impl AsyncBufRead, impl AsyncWrite) {
        let WebsocketIO { ws, reader } = self;
        (reader, WebsocketWriter { ws })
    }
}

impl WebsocketReader {
    fn write_remaining(&mut self, buf: &mut [u8]) -> usize {
        match self.remaining.len().cmp(&buf.len()) {
            Ordering::Less => {
                let amount = self.remaining.len();
                buf[0..amount].copy_from_slice(&self.remaining);
                self.remaining.clear();
                amount
            }
            Ordering::Equal => {
                buf.copy_from_slice(&self.remaining);
                self.remaining.clear();
                buf.len()
            }
            Ordering::Greater => {
                let amount = buf.len();
                buf.copy_from_slice(&self.remaining[..amount]);
                self.remaining.drain(0..amount);
                amount
            }
        }
    }
}

impl AsyncRead for WebsocketReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if !self.remaining.is_empty() {
            return Poll::Ready(Ok(self.write_remaining(buf)));
        }

        let array = match Pin::new(&mut self.read_rx).poll_next(cx) {
            Poll::Ready(Some(item)) => item,
            Poll::Ready(None) => return Poll::Pending,
            Poll::Pending => return Poll::Pending,
        };

        let array_length = array.length() as usize;

        let read = match array_length.cmp(&buf.len()) {
            Ordering::Equal => {
                array.copy_to(buf);
                buf.len()
            }
            Ordering::Less => {
                array.copy_to(&mut buf[..array_length]);
                array_length
            }
            Ordering::Greater => {
                self.remaining.resize(array_length, 0);
                array.copy_to(self.as_mut().remaining.as_mut_slice());

                self.write_remaining(buf)
            }
        };

        Poll::Ready(Ok(read))
    }
}
impl AsyncBufRead for WebsocketReader {
    fn poll_fill_buf(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<futures_io::Result<&[u8]>> {
        if !self.remaining.is_empty() {
            return Poll::Ready(Ok(self.get_mut().remaining.as_slice()));
        }

        let array = match Pin::new(&mut self.read_rx).poll_next(cx) {
            Poll::Ready(Some(item)) => item,
            Poll::Ready(None) => return Poll::Pending,
            Poll::Pending => return Poll::Pending,
        };

        self.remaining.extend(&array.to_vec());

        if self.remaining.len() == 0 {
            return Poll::Pending;
        }
        Poll::Ready(Ok(self.get_mut().remaining.as_slice()))
    }

    fn consume(mut self: std::pin::Pin<&mut Self>, amt: usize) {
        if self.remaining.len() == amt {
            self.remaining.clear();
            return;
        }
        self.remaining.drain(0..amt);
    }
}

impl AsyncWrite for WebsocketWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.ws.send_with_u8_array(buf).unwrap();

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.ws.close().unwrap();
        Poll::Ready(Ok(()))
    }
}
