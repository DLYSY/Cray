use std::convert::Infallible;
use std::net::SocketAddr;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio_util::io::StreamReader;
use uuid::Uuid;

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
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(fetch))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn fetch(
    client_body: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let a = Mutex::new(StreamReader::new(
        client_body
            .map_err(|e| std::io::Error::other(e))
            .into_data_stream(),
    ));
    get_vless(&mut a);
    let b = a.read_u8().await.unwrap();
    println!("{:?}", b);
    Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
}

async fn get_vless(client_body: Mutex<impl tokio::io::AsyncRead>) -> Vless {
    let mut client_body = client_body.get_mut();
    let vless_version = client_body.read_u8().await.unwrap();
    let uuid = Uuid::from_u128(client_body.read_u128().await.unwrap()).unwrap();
    let proto_len = client_body.read_u8().await.unwrap();
    let mut proto = Vec::new();
    for _ in 0..proto_len {
        proto.push(client_body.read_u8().await.unwrap());
    }
}
