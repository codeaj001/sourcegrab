use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use dialoguer::{Input, MultiSelect};
use indicatif::{ProgressBar, ProgressStyle};

pub fn run_cli() {
    println!("\nWelcome to SourceGrab");

    let mut downloads = Vec::new();
    let format_options = vec!["mp4", "mp3", "webm"];

    loop {
        let url_input: String = Input::new()
            .with_prompt("Enter video URL (or press ENTER to finish)")
            .allow_empty(true)
            .interact_text()
            .unwrap();

        if url_input.trim().is_empty() {
            break;
        }

        let selections = MultiSelect::new()
            .with_prompt("Select one or more formats")
            .items(&format_options)
            .interact()
            .unwrap();

        if selections.is_empty() {
            println!("No format selected. Skipping this URL.");
            continue;
        }

        for i in selections {
            downloads.push((url_input.clone(), format_options[i].to_string()));
        }
    }

    if downloads.is_empty() {
        println!("No downloads to process. Exiting.");
        return;
    }

    let output_dir: String = Input::new()
        .with_prompt("Enter output directory")
        .default("./downloads".into())
        .interact_text()
        .unwrap();

    fs::create_dir_all(&output_dir).unwrap();

    for (i, (url, format)) in downloads.iter().enumerate() {
        println!("\n[{}/{}] Downloading {} as {}", i + 1, downloads.len(), url, format);

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner} {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_message("Downloading...");

        let mut cmd = Command::new("yt-dlp");

        match format.as_str() {
            "mp3" => {
                cmd.args([
                    "-x",
                    "--audio-format",
                    "mp3",
                    "--output",
                    &format!("{}/%(title)s.%(ext)s", output_dir),
                    url,
                ]);
            }
            "mp4" => {
                cmd.args([
                    "-f",
                    "bestvideo[ext=mp4]+bestaudio[ext=m4a]/mp4",
                    "--merge-output-format",
                    "mp4",
                    "--output",
                    &format!("{}/%(title)s.%(ext)s", output_dir),
                    url,
                ]);
            }
            "webm" => {
                cmd.args([
                    "-f",
                    "webm",
                    "--output",
                    &format!("{}/%(title)s.%(ext)s", output_dir),
                    url,
                ]);
            }
            _ => {
                pb.finish_with_message("Unsupported format.");
                continue;
            }
        }

        let status = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();

        pb.finish_and_clear();

        match status {
            Ok(s) if s.success() => println!("Done!"),
            _ => println!("Failed to download."),
        }
        thread::sleep(Duration::from_millis(300));
    }

    println!("\nAll done!");
}
