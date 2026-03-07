use futures_util::TryStreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Frame};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use regex::Regex;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::LazyLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::io::{ReaderStream, StreamReader};
use uuid::Uuid;

use parking_lot::Mutex;
use rand::Rng;
use rand_core::SeedableRng;
use rand_xoshiro::SplitMix64;

use url::Host;

mod init_conf;
mod outbound;

#[allow(unused)]
#[derive(Debug)]
struct Vless {
    version: u8,
    uuid: Uuid,
    proto_len: u8,
    proto: Vec<u8>,
    cmd: u8, // 1: TCP, 2: UDP
    port: u16,
    addr_type: u8, // 0: ipv4, 3: ipv6, 2: domain
    address: Host,
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
        
        let address = if cmd == 1 && (addr_type == 0 || addr_type == 2) {
            let addr_len = inbound_body.read_u8().await?;
            let mut address = vec![0; addr_len as usize];
            inbound_body.read_exact(&mut address).await?;
            Host::parse(String::from_utf8(address)?.as_str())?
        } else if addr_type == 3 {
            Host::Ipv6(inbound_body.read_u128().await?.into())
        } else if cmd ==2 {
            Host::Ipv4(inbound_body.read_u32().await?.into())
        } else {
            unreachable!()
        };
        // let address = match cmd {
        //     1 => {
        //         let addr_len = inbound_body.read_u8().await?;
        //         let mut address = vec![0; addr_len as usize];
        //         inbound_body.read_exact(&mut address).await?;
        //         String::from_utf8(address)?
        //     }
        //     2 => {
        //         format!("{}.{}.{}.{}", inbound_body.read_u8().await?, inbound_body.read_u8().await?, inbound_body.read_u8().await?, inbound_body.read_u8().await?)
        //     }
        //     _ => {
        //         unreachable!()
        //     }
        // };

        Ok(Vless {
            version: vless_version,
            uuid,
            proto_len,
            proto: proto_buf,
            cmd,
            port,
            addr_type,
            address,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_conf::init_config()?;
    let listener = TcpListener::bind(SocketAddr::from((
        [127, 0, 0, 1],
        init_conf::CONFIG.get().unwrap().port,
    )))
    .await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

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
    static RE_X_PADDING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r".*\?x_padding=X{100,1000}").unwrap());

    if !client_body
        .headers()
        .get("Referer")
        .map(|i| RE_X_PADDING.is_match(i.to_str().unwrap()))
        .unwrap_or(false)
    {
        println!("{:#?}", client_body.headers());
        return Ok(Response::builder().status(hyper::StatusCode::OK).body(
            Full::new(Bytes::from("Hello World"))
                .map_err(|e| std::io::Error::other(e))
                .boxed(),
        )?);
    }

    let mut inbound_stream = Box::pin(StreamReader::new(
        client_body
            .map_err(|e| std::io::Error::other(e))
            .into_data_stream(),
    ));

    let vless_header = Vless::new(inbound_stream.as_mut()).await?;

    if vless_header.uuid != init_conf::CONFIG.get().unwrap().uuid {
        println!("incoming uuid: {:?}", vless_header.uuid);
        return Ok(Response::builder()
            .status(hyper::StatusCode::FORBIDDEN)
            .body(
                Full::new(Bytes::from("UUID Mismatch"))
                    .map_err(|e| std::io::Error::other(e))
                    .boxed(),
            )?);
    }

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
    static RNG: LazyLock<Mutex<SplitMix64>> =
        LazyLock::new(|| Mutex::new(SplitMix64::from_os_rng()));
    let res = hyper::Response::builder()
        .status(hyper::StatusCode::OK)
        .header("X-Padding", "X".repeat(RNG.lock().random_range(100..1000)))
        .body(stream_body)?;

    Ok(res)
}
