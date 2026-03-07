use tokio::net::TcpStream;

pub async fn tcp_outbound(
    inbound_stream: &mut impl tokio::io::AsyncRead,
    address: url::Host,
    port: u16,
) -> tokio::io::DuplexStream {
    let (mut outbound_reader, mut outbound_writer) =
        TcpStream::connect(format!("{}:{}", address, port))
            .await
            .unwrap()
            .into_split();

    let (mut response_writer, response_reader) = tokio::io::duplex(64 * 1024);
    response_reader
}

pub fn udp_outbound(
    inbound_stream: &mut impl tokio::io::AsyncRead,
    address: url::Host,
    port: u16,
) -> tokio::io::DuplexStream {
    let (mut response_writer, response_reader) = tokio::io::duplex(64 * 1024);
    response_reader
}
