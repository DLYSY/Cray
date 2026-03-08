use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use futures_util::TryStreamExt;
use log::info;
use regex::Regex;
use std::pin::Pin;
use std::sync::LazyLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio_util::io::{ReaderStream, StreamReader};
use url::Host;
use uuid::Uuid;

mod init_conf;

#[allow(unused)]
#[derive(Debug, Clone)]
struct Vless {
    version: u8,
    uuid: Uuid,
    proto_len: u8,
    proto: Vec<u8>,
    cmd: u8, // 1: TCP, 2: UDP
    port: u16,
    addr_type: u8, // 0: ipv4(String), 1: ipv4(Hex), 2: domain(String), 3: ipv6(Hex)
    address: Host,
    udp_len: Option<u16>,
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

        let address = if addr_type == 0 || addr_type == 2 {
            let addr_len = inbound_body.read_u8().await?;
            let mut address = vec![0; addr_len as usize];
            inbound_body.read_exact(&mut address).await?;
            Host::parse(String::from_utf8(address)?.as_str())?
        } else if addr_type == 1 {
            Host::Ipv4(inbound_body.read_u32().await?.into())
        } else if addr_type == 3 {
            Host::Ipv6(inbound_body.read_u128().await?.into())
        } else {
            unreachable!()
        };

        let udp_len = if cmd == 2 {
            Some(inbound_body.read_u16().await?)
        } else {
            None
        };

        Ok(Vless {
            version: vless_version,
            uuid,
            proto_len,
            proto: proto_buf,
            cmd,
            port,
            addr_type,
            address,
            udp_len,
        })
    }
}

#[post("/")]
async fn fetch(
    req: actix_web::HttpRequest,
    body: actix_web::web::Payload,
) -> std::io::Result<impl Responder> {
    static RE_X_PADDING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r".*\?x_padding=X{100,1000}").unwrap());

    if !req
        .headers()
        .get("Referer")
        .map(|i| RE_X_PADDING.is_match(i.to_str().unwrap()))
        .unwrap_or(false)
    {
        println!(
            "client {}, x_padding verification failed",
            req.peer_addr().unwrap()
        );
        return Ok(HttpResponse::Unauthorized().body("Header Verification Failed"));
    }

    let mut inbound_stream = Box::pin(StreamReader::new(
        body.map_err(|e| std::io::Error::other(e)),
    ));

    let Ok(vless_header) = Vless::new(inbound_stream.as_mut()).await else {
        return Ok(HttpResponse::BadRequest().finish());
    };

    if vless_header.uuid != init_conf::CONFIG.get().unwrap().uuid {
        println!(
            "client {}, uuid verification failed",
            req.peer_addr().unwrap()
        );
        return Ok(HttpResponse::Unauthorized().body("UUID verification failed"));
    }

    println!(
        "{}->{}://{}:{}",
        req.peer_addr().unwrap(),
        if vless_header.cmd == 1 { "tcp" } else { "udp" },
        vless_header.address,
        vless_header.port
    );

    let mut response = HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("X-Padding", "X".repeat(rand::random_range(100..1000))))
        .insert_header(("X-Accel-Buffering", "no"))
        .insert_header(("Cache-Control", "no-cache"))
        .take();

    if vless_header.cmd == 1 {
        // TCP
        let (mut outbound_reader, mut outbound_writer) =
            TcpStream::connect(format!("{}:{}", vless_header.address, vless_header.port))
                .await?
                .into_split();

        let (mut response_writer, response_reader) = tokio::io::duplex(64 << 10);

        response_writer
            .write_all(&[vless_header.version, 0])
            .await?;

        actix_web::rt::spawn(async move {
            tokio::try_join!(
                tokio::io::copy(&mut inbound_stream, &mut outbound_writer),
                tokio::io::copy(&mut outbound_reader, &mut response_writer)
            )
        });
        return Ok(response.streaming(ReaderStream::new(response_reader)));
    } else {
        // UDP
        let udp_socket = UdpSocket::bind("0.0.0.0:0").await?;
        udp_socket
            .connect(format!("{}:{}", vless_header.address, vless_header.port))
            .await?;
        let mut send_bytes = vec![0; vless_header.udp_len.unwrap() as usize];
        inbound_stream.read(&mut send_bytes).await?;

        udp_socket.send(&send_bytes).await?;

        let mut recv_bytes = [0; 64 << 10];

        let recv_len = udp_socket.recv(&mut recv_bytes).await?;

        let response_bytes = [
            vec![vless_header.version, 0],
            { recv_len as u16 }.to_be_bytes().to_vec(),
            recv_bytes[..recv_len].to_vec(),
        ]
        .concat();

        return Ok(response.body(response_bytes));
    };
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_conf::init_config().unwrap();
    let config = init_conf::CONFIG.get().unwrap();
    HttpServer::new(|| App::new().service(fetch))
        .bind(("0.0.0.0", config.port))?
        .run()
        .await
}
