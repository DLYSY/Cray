FROM scratch

ADD ./target/x86_64-unknown-linux-musl/release/Cray /app/Cray

ENTRYPOINT ["/app/Cray"]