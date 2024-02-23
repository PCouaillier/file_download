use hyper::body::Bytes;
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;
use http_body_util::{BodyExt, Empty};
use tokio::io::AsyncWriteExt as _;

#[cfg(feature = "async-std")]
use async_std::{
    path::Path,
    io::{BufWriter, File},
    net::TcpStream,
    prelude::*
};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use tokio::{
    fs::File,
    io::BufWriter,
    net::TcpStream,
};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use std::path::Path;

// A simple type alias so as to DRY.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;


pub(crate) async fn download_http1(url: &str, target_path: &Path) -> Result<()> {
    let uri = Uri::try_from(url)?;
    let host = uri.host().expect("uri has no host");
    let port = uri.port_u16().unwrap_or(443);
    if port == 443 {
        panic!("hyper + https not supported yet")
    }
    let stream = TcpStream::connect((host, port)).await?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    conn.await?;

    let authority = uri.authority().unwrap().clone();

    let path = uri.path();
    let req = Request::builder()
        .uri(path)
        .header(hyper::header::HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;

    let mut res = sender.send_request(req).await?;

    // Stream the body, writing each chunk to stdout as we get it
    // (instead of buffering and printing at the end).
    let mut file = BufWriter::new(File::open(target_path).await?);
    
    while let Some(next) = res.frame().await {
        let frame = next?;
        if let Some(chunk) = frame.data_ref() {
            file.write(&chunk).await?;
        }
    }

    Ok(())
}
