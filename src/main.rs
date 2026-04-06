mod cli;

use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MessageId},
    utils::command::BotCommands,
};
use std::error::Error;
use tokio::process::Command as TokioCommand;
use uuid::Uuid;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use regex::Regex;

type MyDialogue = Dialogue<State, InMemStorage<State>>;
type HandlerResult = Result<(), Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct DownloadsMap(
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<ChatId, 
    tokio::task::AbortHandle>>>
);

impl Default for DownloadsMap {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())))
    }
}

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Start,
    SelectQuality { url: String },
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Help")]
    Help,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting bot...");

    let token = std::env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to build client");

    let bot = Bot::with_client(token, client);

    let active_downloads = DownloadsMap::default();

    let schema = dptree::entry()
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, InMemStorage<State>, State>()
                .branch(
                    dptree::entry()
                        .filter_command::<Command>()
                        .endpoint(answer_command)
                )
                .branch(dptree::case![State::Start].endpoint(handle_url))
        )
        .branch(
            Update::filter_callback_query()
                .branch(dptree::filter(|q: CallbackQuery| q.data == Some("cancel".to_string()))
                    .endpoint({
                        let active_downloads = active_downloads.clone();
                        move |bot: Bot, q: CallbackQuery| {
                            let active_downloads = active_downloads.clone();
                            async move {
                                handle_cancel(bot, q, active_downloads).await
                            }
                        }
                    }))
                .enter_dialogue::<CallbackQuery, InMemStorage<State>, State>()
                .branch(dptree::case![State::SelectQuality { url }].endpoint(handle_quality_selection))
        );

    Dispatcher::builder(bot, schema)
        .dependencies(dptree::deps![InMemStorage::<State>::new(), active_downloads])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn answer_command(bot: Bot, msg: Message, cmd: Command, dialogue: MyDialogue) -> HandlerResult {
    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "Welcome! Send me a video URL to download.").await?;
            dialogue.update(State::Start).await?;
        }
        Command::Help => {
            bot.send_message(msg.chat.id, "Just send a URL. I'll ask for quality.").await?;
        }
    }
    Ok(())
}

async fn handle_url(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    if let Some(text) = msg.text() {
        if text.starts_with("http") {
            let url = text.to_string();
            
            let keyboards = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback("480p (Default)", "480p"),
                    InlineKeyboardButton::callback("720p", "720p"),
                ],
                vec![
                    InlineKeyboardButton::callback("1080p", "1080p"),
                    InlineKeyboardButton::callback("Audio (MP3)", "mp3"),
                ],
            ]);

            bot.send_message(msg.chat.id, "Select download quality:")
                .reply_markup(keyboards)
                .await?;
            
            dialogue.update(State::SelectQuality { url }).await?;
        } else {
            bot.send_message(msg.chat.id, "Please send a valid URL starting with http.").await?;
        }
    }
    Ok(())
}

async fn handle_quality_selection(
    bot: Bot,
    q: CallbackQuery,
    dialogue: MyDialogue,
    url: String, // Extracted from State::SelectQuality
    active_downloads: DownloadsMap,
) -> HandlerResult {
    if let Some(quality) = q.data {
        bot.answer_callback_query(q.id.clone()).await?;
        
        if let Some(message) = q.message {
            let cancel_keyboard = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback("Cancel", "cancel")
            ]]);

            bot.edit_message_text(message.chat.id, message.id, format!("Downloading {}...", quality))
                .reply_markup(cancel_keyboard)
                .await?;

            let chat_id = message.chat.id;
            let bot_clone = bot.clone();
            let active_downloads_clone = active_downloads.clone();
            
            // Spawn the download task to avoid blocking
            let join_handle = tokio::spawn(async move {
                if let Err(e) = process_download(bot_clone, chat_id, message.id, url, quality, active_downloads_clone).await {
                    log::error!("Download failed: {}", e);
                }
            });

            active_downloads.0.lock().unwrap().insert(chat_id, join_handle.abort_handle());
            
            // Reset dialogue to start
            dialogue.update(State::Start).await?;
        }
    }
    Ok(())
}

async fn handle_cancel(
    bot: Bot,
    q: CallbackQuery,
    active_downloads: DownloadsMap,
) -> HandlerResult {
    if let Some(message) = q.message {
        let chat_id = message.chat.id;
        let handle_opt = active_downloads.0.lock().unwrap().remove(&chat_id);
        
        if let Some(handle) = handle_opt {
            handle.abort();
            bot.edit_message_text(chat_id, message.id, "Download cancelled.").await?;
        } else {
             bot.answer_callback_query(q.id).text("No active download found.").await?;
        }
    }
    Ok(())
}

struct DownloadGuard {
    active_downloads: DownloadsMap,
    chat_id: ChatId,
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.active_downloads.0.lock().unwrap().remove(&self.chat_id);
    }
}

async fn process_download(bot: Bot, chat_id: ChatId, message_id: MessageId, url: String, quality: String, active_downloads: DownloadsMap) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _guard = DownloadGuard { active_downloads: active_downloads.clone(), chat_id };
    let uuid = Uuid::new_v4();
    let output_template = format!("downloads/{}_%(title)s.%(ext)s", uuid);
    
    // Ensure downloads directory exists
    tokio::fs::create_dir_all("downloads").await?;

    let mut cmd = if std::path::Path::new("./yt-dlp").exists() {
        TokioCommand::new("./yt-dlp")
    } else {
        TokioCommand::new("yt-dlp")
    };
    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::piped());
    // We don't pipe stderr to let it flow or we could pipe it too if we want to debug.
    
    // Add common arguments
    cmd.arg("--newline"); // Essential for parsing progress line-by-line

    match quality.as_str() {
        "mp3" => {
            cmd.args(&[
                "-x",
                "--audio-format", "mp3",
                "--output", &output_template,
                &url
            ]);
        },
        "480p" => {
            cmd.args(&[
                "-f", "bestvideo[height<=480]+bestaudio/best[height<=480]/best",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        "720p" => {
            cmd.args(&[
                "-f", "bestvideo[height<=720]+bestaudio/best[height<=720]/best",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        "1080p" => {
            cmd.args(&[
                "-f", "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        _ => {
            // Default to 480p
             cmd.args(&[
                "-f", "bestvideo[height<=480]+bestaudio/best[height<=480]/best",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        }
    }

    // We need to get the filename to send it.
    // yt-dlp has --print filename, but we are running the download.
    // We can use --print filename --no-simulate to get it? No.
    // We can list the file after download matching the UUID.
    
    // Spawn command
    let mut child = cmd.spawn()?;
    
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout).lines();
    
    // Progress loop and cancel listener
    // We need to keep processing lines.
    // We should update the message periodically.
    
    let re = Regex::new(r"\[download\]\s+(\d+\.?\d*)%").unwrap();
    let mut last_update_time = std::time::Instant::now();
    let mut last_percent = 0u8;

    // We need to handle the child process finishing.
    loop {
        tokio::select! {
             line = reader.next_line() => {
                match line {
                    Ok(Some(line_text)) => {
                         if let Some(caps) = re.captures(&line_text) {
                            if let Ok(pct_f) = caps[1].parse::<f32>() {
                                let pct = pct_f as u8;
                                // Update only if > 0 and (enough time passed or significant progress)
                                // Telegram rate limits are strict.
                                if pct != last_percent && (last_update_time.elapsed().as_secs() >= 2 || pct == 100 || (pct % 10 == 0 && pct != last_percent)) {
                                     last_percent = pct;
                                     last_update_time = std::time::Instant::now();
                                     
                                     let progress_bar = draw_progress_bar(pct);
                                     let cancel_keyboard = InlineKeyboardMarkup::new(vec![vec![
                                        InlineKeyboardButton::callback("Cancel", "cancel")
                                     ]]);
                                     
                                     // Ignore errors (e.g. if unchanged)
                                     let _ = bot.edit_message_text(chat_id, message_id, format!("Downloading {}...\n{}", quality, progress_bar))
                                        .reply_markup(cancel_keyboard)
                                        .await;
                                }
                            }
                         }
                    }
                    Ok(None) => break, // EOF
                    Err(_) => break, // Error or end
                }
             }
             _ = child.wait() => {
                 break;
             }
        }
    }
    
    // Ensure it's finished
    // The previous loop might exit on EOF before wait() returns, or vice versa
    // Actually child.wait() consumes the child but we don't have ownership in select! easily without complications.
    // Easier: Just read stdout until EOF (which happens when child closes stdout), then wait on child.
    
    // Re-implementation of loop correctly:
    // Actually the previous select has issue: child.wait() borrows child mutably.
    // We can just read lines until None. When stdout closes, process is likely done or dead.
    // Then checking status.
    
    // Simple reader loop:
    // while let Ok(Some(line)) = reader.next_line().await { ... }
    
     // Let's rely on reader EOF. If process stays alive but closes stdout, we continue to wait status.
    
    // No, let's use the first loop approach but fix it. Using a separate task or just treating stdout EOF as end of progress.
    // `process_download` owns `child` now.
    
    // Let's rewrite the waiting part below.
    let status = child.wait().await?;
    
    if !status.success() {
        bot.send_message(chat_id, "Download failed.").await?;
        return Ok(());
    }

    // Find the file
    let mut dir = tokio::fs::read_dir("downloads").await?;
    let mut downloaded_file = None;
    
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if let Some(name) = path.file_name() {
            if name.to_string_lossy().starts_with(&uuid.to_string()) {
                downloaded_file = Some(path);
                break;
            }
        }
    }

    if let Some(path) = downloaded_file {
        bot.send_message(chat_id, "Uploading...").await?;
        
        if quality == "mp3" {
             bot.send_audio(chat_id, InputFile::file(&path)).await?;
        } else {
             bot.send_video(chat_id, InputFile::file(&path)).await?;
        }
        
        // Clean up
        tokio::fs::remove_file(path).await?;
    } else {
        bot.send_message(chat_id, "Could not find downloaded file.").await?;
    }

    Ok(())
}
fn draw_progress_bar(percent: u8) -> String {
    let width = 10;
    let filled = (percent as f32 / 100.0 * width as f32).round() as usize;
    let empty = width - filled;
    
    let fill_char = "▓";
    let empty_char = "░";
    
    let bar: String = (0..filled).map(|_| fill_char).collect::<String>() 
                    + &(0..empty).map(|_| empty_char).collect::<String>();
                    
    format!("[{}] {}%", bar, percent)
}
