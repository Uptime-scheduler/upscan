FROM rust:slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:12-slim
COPY --from=builder /build/target/release/upscan /usr/local/bin/upscan
COPY docker/cargo/smoke_test.sh /smoke_test.sh
RUN chmod +x /smoke_test.sh
CMD ["/smoke_test.sh"]
