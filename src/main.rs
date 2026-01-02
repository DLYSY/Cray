use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Frame};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::io::{ReaderStream, StreamReader};
use uuid::Uuid;

#[allow(unused)]
#[derive(Debug)]
struct Vless {
    version: u8,
    uuid: Uuid,
    proto_len: u8,
    proto: Vec<u8>,
    cmd: u8,
    port: u16,
    addr_type: u8,
    addr_len: u8,
    address: String,
}

impl Vless {
    async fn new<R>(mut inbound_body: Pin<&mut R>) -> anyhow::Result<Vless>
    where
        R: tokio::io::AsyncRead + ?Sized,
    {
        let vless_version = inbound_body.read_u8().await?;
        let uuid = Uuid::from_u128(inbound_body.read_u128().await?);
        let proto_len = inbound_body.read_u8().await?;
        let mut proto_buf = vec![0; proto_len as usize];
        inbound_body.read_exact(&mut proto_buf).await?;
        let cmd = inbound_body.read_u8().await?;
        let port = inbound_body.read_u16().await?;
        let addr_type = inbound_body.read_u8().await?;
        let addr_len = inbound_body.read_u8().await?;
        let mut address = vec![0; addr_len as usize];
        inbound_body.read_exact(&mut address).await?;
        let address = String::from_utf8(address)?;

        Ok(Vless {
            version: vless_version,
            uuid,
            proto_len,
            proto: proto_buf,
            cmd,
            port,
            addr_type,
            addr_len,
            address,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            let builder =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
            if let Err(e) = builder.serve_connection(io, service_fn(fetch)).await {
                eprintln!("Error serving connection: {:?}", e);
            }
        });
    }
}

async fn fetch(
    client_body: Request<hyper::body::Incoming>,
) -> anyhow::Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let mut inbound_stream = Box::pin(StreamReader::new(
        client_body
            .map_err(|e| std::io::Error::other(e))
            .into_data_stream(),
    ));

    let vless_header = Vless::new(inbound_stream.as_mut()).await?;
    println!("{:?}", vless_header);

    let (mut outbound_reader, mut outbound_writer) =
        TcpStream::connect(format!("{}:{}", vless_header.address, vless_header.port))
            .await?
            .into_split();

    let (mut response_writer, response_reader) = tokio::io::duplex(64 * 1024);

    let stream_body =
        http_body_util::StreamBody::new(ReaderStream::new(response_reader).map_ok(Frame::data))
            .boxed();

    response_writer
        .write_all(&[vless_header.version, 0])
        .await?;

    tokio::spawn(async move {
        tokio::try_join!(
            tokio::io::copy(&mut inbound_stream, &mut outbound_writer),
            tokio::io::copy(&mut outbound_reader, &mut response_writer)
        )
    });

    let res = hyper::Response::builder()
        .status(hyper::StatusCode::OK)
        .body(stream_body)?;

    Ok(res)
}
