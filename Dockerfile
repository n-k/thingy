# Build Stage
FROM clux/muslrust AS builder
WORKDIR /usr/src/thingy
COPY . .
RUN cargo install --target x86_64-unknown-linux-musl --path .

# Bundle Stage
FROM alpine
WORKDIR /app
COPY --from=builder /root/.cargo/bin/thingy .
ENV LISTEN_ADDRESS=0.0.0.0
WORKDIR /workspace
EXPOSE 8080
USER 1000
CMD ["/app/thingy", "/workspace"]
