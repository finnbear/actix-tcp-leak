FROM rust:1.55-bullseye

RUN apt update
RUN apt install net-tools

WORKDIR /root

COPY Cargo.toml Cargo.toml
COPY src/* src/

RUN cargo build

CMD ["./target/debug/tcp-leak"]