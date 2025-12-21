mod cli;

use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile},
    utils::command::BotCommands,
};
use std::error::Error;
use tokio::process::Command as TokioCommand;
use uuid::Uuid;

type MyDialogue = Dialogue<State, InMemStorage<State>>;
type HandlerResult = Result<(), Box<dyn Error + Send + Sync>>;

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
    pretty_env_logger::init();
    log::info!("Starting bot...");

    let token = std::env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to build client");

    let bot = Bot::with_client(token, client);

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
                .enter_dialogue::<CallbackQuery, InMemStorage<State>, State>()
                .branch(dptree::case![State::SelectQuality { url }].endpoint(handle_quality_selection))
        );

    Dispatcher::builder(bot, schema)
        .dependencies(dptree::deps![InMemStorage::<State>::new()])
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
) -> HandlerResult {
    if let Some(quality) = q.data {
        bot.answer_callback_query(q.id.clone()).await?;
        
        if let Some(message) = q.message {
            bot.edit_message_text(message.chat.id, message.id, format!("Downloading {}...", quality))
                .await?;

            let chat_id = message.chat.id;
            let bot_clone = bot.clone();
            
            // Spawn the download task to avoid blocking
            tokio::spawn(async move {
                if let Err(e) = process_download(bot_clone, chat_id, url, quality).await {
                    log::error!("Download failed: {}", e);
                }
            });
            
            // Reset dialogue to start
            dialogue.update(State::Start).await?;
        }
    }
    Ok(())
}

async fn process_download(bot: Bot, chat_id: ChatId, url: String, quality: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    let uuid = Uuid::new_v4();
    let output_template = format!("downloads/{}_%(title)s.%(ext)s", uuid);
    
    // Ensure downloads directory exists
    tokio::fs::create_dir_all("downloads").await?;

    let mut cmd = TokioCommand::new("yt-dlp");
    
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
                "-f", "bestvideo[height<=480]+bestaudio/best[height<=480]",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        "720p" => {
            cmd.args(&[
                "-f", "bestvideo[height<=720]+bestaudio/best[height<=720]",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        "1080p" => {
            cmd.args(&[
                "-f", "bestvideo[height<=1080]+bestaudio/best[height<=1080]",
                "--merge-output-format", "mp4",
                "--output", &output_template,
                &url
            ]);
        },
        _ => {
            // Default to 480p
             cmd.args(&[
                "-f", "bestvideo[height<=480]+bestaudio/best[height<=480]",
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
    
    let status = cmd.status().await?;
    
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