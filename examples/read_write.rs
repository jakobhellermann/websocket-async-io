use futures_util::io::AsyncBufReadExt;
use futures_util::io::AsyncWriteExt;
use futures_util::AsyncReadExt;
use wasm_bindgen::prelude::*;
use websocket_async_io::WebsocketIO;

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async move {
        console_log!("AsyncRead:");
        run().await.unwrap();
        console_log!("AsyncBufRead:");
        run_buf_read().await.unwrap();
    });
    Ok(())
}

async fn run() -> Result<(), std::io::Error> {
    let ws = WebsocketIO::new("localhost:8000").await?;
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

async fn run_buf_read() -> Result<(), std::io::Error> {
    let ws = WebsocketIO::new("localhost:8000").await?;
    let (mut reader, mut writer) = ws.split();

    writer.write_all(&[0, 1, 2, 3]).await?;
    writer.write_all(&[42, 34]).await?;
    writer.write_all(&[0, 0, 1, 2]).await?;
    writer.flush().await?;

    let mut buf = vec![0; 1024];
    for _ in 0..3 {
        let read = reader.read(&mut buf).await?;
        console_log!("{:?}", &buf[0..read]);
    }

    Ok(())
}
