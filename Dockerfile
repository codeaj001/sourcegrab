# Build stage
FROM rust:latest as builder

WORKDIR /app
COPY . .

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev

# Build the release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
# yt-dlp requires python3
# ffmpeg is often needed for merging video+audio
# ca-certificates for HTTPS
# openssl for the bot
RUN apt-get update && apt-get install -y \
    ca-certificates \
    ffmpeg \
    python3 \
    curl \
    openssl \
    && rm -rf /var/lib/apt/lists/*

# Install yt-dlp directly to ensure latest version
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/urlvideodownloader /app/urlvideodownloader

# Create downloads directory
RUN mkdir downloads

# Run the binary
CMD ["./urlvideodownloader"]
