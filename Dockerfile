# Stage 1: Build the Rust binary with musl
FROM rust:1.86 AS builder
# Install musl-dev for musl-gcc
RUN apt-get update && apt-get install -y musl-dev musl-tools nfs-common
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /usr/src/pvtracker
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: Create the runtime image
FROM alpine:3.18
RUN apk add --no-cache ca-certificates
COPY --from=builder /usr/src/pvtracker/target/x86_64-unknown-linux-musl/release/pvtracker /usr/local/bin/pvtracker
ENTRYPOINT ["/usr/local/bin/pvtracker"]