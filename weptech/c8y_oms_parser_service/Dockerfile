# Enable BuildKit build options
FROM rust:slim AS builder
WORKDIR /usr/src/c8y-oms-parser

# Copy all source files
COPY . .

# Build release binary using persistent BuildKit cache locations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/c8y-oms-parser/target \
    cargo build --release && \
    cp /usr/src/c8y-oms-parser/target/release/c8y_oms_parser_service /usr/local/bin/c8y-oms-parser

# Runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/c8y-oms-parser /usr/local/bin/c8y-oms-parser

EXPOSE 80
CMD ["c8y-oms-parser"]