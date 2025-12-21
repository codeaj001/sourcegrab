# URL Video Downloader Bot

A powerful, fast, and feature-rich Telegram bot written in Rust that allows users to download videos and audio from various platforms (YouTube, TikTok, Twitter, etc.) simply by sending a link.

## Highlights

- **High Performance.** Built on top of `teloxide` and `tokio`, this bot handles multiple downloads asynchronously with ease, ensuring the bot remains responsive even during heavy loads.
- **Quality Control.** Users are not stuck with a default. The bot uses interactive dialogues to let users choose between **1080p**, **720p**, **480p**, or **MP3** audio extraction.
- **Broad Support.** Powered by `yt-dlp`, it supports downloading from hundreds of websites out of the box.
- **Privacy Focused.** No logs are kept, and files are processed temporarily and deleted immediately after upload.

## Setting up your environment

1. **Download Rust.**
   Ensure you have the latest stable version of Rust installed.
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Install External Dependencies.**
   The bot relies on `yt-dlp` for downloading and `ffmpeg` for processing.
   ```bash
   # Linux (Debian/Ubuntu)
   sudo apt install ffmpeg python3
   sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
   sudo chmod a+rx /usr/local/bin/yt-dlp
   ```

3. **Create a Telegram Bot.**
   Talk to [@BotFather](https://t.me/BotFather) to create a new bot and get your token.

4. **Initialize the Token.**
   Set the `TELOXIDE_TOKEN` environment variable:
   ```bash
   # Unix-like
   export TELOXIDE_TOKEN=<Your token here>
   
   # Windows PowerShell
   $env:TELOXIDE_TOKEN="<Your token here>"
   ```

5. **Run the Bot.**
   ```bash
   cargo run
   ```

## Usage

**Commands**
Commands are strongly typed and parsed automatically.
- `/start` - Initialize the bot and welcome message.
- `/help` - Display help information.

**Downloading**
1. Send a video URL (e.g., from YouTube or TikTok) to the bot.
2. The bot will present an interactive menu.
3. Select your desired quality (e.g., `1080p` or `Audio (MP3)`).
4. The bot downloads, processes, and sends the file to you.

## Deployment (Docker)

This project includes a `Dockerfile` for easy deployment on platforms like Railway or Fly.io.

```bash
# Build
docker build -t video-bot .

# Run
docker run -d -e TELOXIDE_TOKEN=<your_token> video-bot
```

## Privacy Policy

1. **Data Collection**: We do not store any user data, logs, or download history.
2. **File Handling**: Videos and audio files are processed temporarily on our servers for the purpose of downloading and sending them to you. They are automatically deleted immediately after being sent.
3. **Open Source**: This bot is open source. You can review the code to verify our privacy claims.

## Contributing

Feel free to open issues or submit PRs to improve the bot!