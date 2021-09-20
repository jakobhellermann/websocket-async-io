# websocket-async-io

Implementations of [`AsyncRead`](https://docs.rs/futures/0.3.17/futures/io/trait.AsyncRead.html) and [`AsyncWrite`](https://docs.rs/futures/0.3.17/futures/io/trait.AsyncWrite.html) on top of websockets using [`web-sys`](https://github.com/rustwasm/wasm-bindgen/tree/master/crates/web-sys))

# Example

```rust
async fn run() -> Result<(), std::io::Error> {
    let ws = WebsocketIO::new(([127, 0, 0, 1], 8000).into()).await?;
    let (mut reader, mut writer) = ws.split();

    writer.write_all(&[0, 1, 2, 3, 93]).await?;
    writer.write_all(&[42, 34, 93]).await?;
    writer.write_all(&[0, 0, 1, 2, 93]).await?;

    let mut buf = Vec::new();
    for _ in 0..3 {
        reader.read_until(93, &mut buf).await?;
        console_log!("{:?}", buf);
        buf.clear();
    }

    Ok(())
}
```