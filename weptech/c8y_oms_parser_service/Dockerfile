FROM rust:slim AS builder
WORKDIR /usr/src/c8y-oms-parser
COPY . .
RUN cargo build --release

# Stage 2: Bundle the compiled binary into a clean, minimal runtime layer
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/c8y-oms-parser/target/release/c8y_oms_parser_service /usr/local/bin/c8y-oms-parser
EXPOSE 80
CMD ["c8y-oms-parser"]