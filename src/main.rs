use std::fs;
use clap::Parser;
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use git2::{Repository, DiffOptions, DiffLine, Delta, ErrorCode};
use std::collections::HashMap;

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const SYSTEM_PROMPT: &str = "You are a helpful assistant that writes clear and concise Git commit messages in the imperative mood, without any speculation.";
const USER_PROMPT_TEMPLATE: &str = "\
Write a Git commit message with a short title and a detailed body, using the imperative mood. Do not include any speculation or guesses. Be concise and precise. Use bullet points in the body to list changes.

Changes:
{structured_changes}
";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the OpenAI API key file
    #[arg(short, long, value_name = "FILE")]
    api_key_path: String,

    /// Additional context for the commit message
    #[arg(short, long, value_name = "CONTEXT")]
    context: Option<String>,

    /// Path to the working directory (defaults to current directory)
    #[arg(short, long, value_name = "DIR", default_value = ".")]
    workdir_path: String,

    /// OpenAI model to use (defaults to gpt-4)
    #[arg(short, long, value_name = "MODEL", default_value = "gpt-4")]
    model: String,
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: String,
}

struct FileChange {
    file_path: String,
    change_type: String,
    summaries: Vec<String>,
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    // Read the API key
    let api_key = match fs::read_to_string(&args.api_key_path) {
        Ok(key) => key.trim().to_string(),
        Err(e) => {
            eprintln!("Failed to read API key from {}: {}", args.api_key_path, e);
            std::process::exit(1);
        }
    };

    // Open the Git repository at the specified working directory path
    let repo = match Repository::open(&args.workdir_path) {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!(
                "Failed to open Git repository at '{}': {}",
                args.workdir_path, e
            );
            std::process::exit(1);
        }
    };

    // Prepare git information
    let structured_changes = get_structured_changes(&repo);
    if structured_changes.is_empty() {
        println!("No changes detected. Nothing to generate a commit message for.");
        std::process::exit(0);
    }

    let prompt = USER_PROMPT_TEMPLATE.replace("{structured_changes}", &structured_changes);

    // Prepare OpenAI API request
    let request_body = OpenAIRequest {
        model: args.model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: prompt,
            },
        ],
    };

    // Create a client with rustls TLS backend
    let client = Client::builder()
        .use_rustls_tls()
        .build()
        .expect("Failed to build HTTP client");

    // Send request to OpenAI API
    let response = client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send();

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                let resp_json: OpenAIResponse = resp.json().unwrap();
                let commit_message = resp_json.choices[0].message.content.trim();
                // Output the commit message without extra text
                println!("{}", commit_message);
            } else {
                eprintln!(
                    "Failed to generate commit message. API response status: {}",
                    resp.status()
                );
                let resp_text = resp.text().unwrap_or_default();
                eprintln!("Response body:\n{}", resp_text);
            }
        }
        Err(e) => {
            eprintln!("Error sending request to OpenAI API: {}", e);
        }
    }
}

fn get_structured_changes(repo: &Repository) -> String {
    let diff = get_combined_diff(repo);
    let changes = collect_changes(&diff);
    format_changes_for_prompt(&changes)
}

fn get_combined_diff(repo: &Repository) -> git2::Diff {
    let mut diff_opts = DiffOptions::new();
    diff_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true);

    if is_initial_commit(repo) {
        repo.diff_index_to_workdir(None, Some(&mut diff_opts))
            .unwrap()
    } else {
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        repo.diff_tree_to_workdir(Some(&head), Some(&mut diff_opts))
            .unwrap()
    }
}

fn collect_changes(diff: &git2::Diff) -> Vec<FileChange> {
    let mut changes_map: HashMap<String, FileChange> = HashMap::new();

    diff.foreach(
        &mut |_delta, _progress| {
            true // No mutation of changes_map here
        },
        None,
        Some(&mut |_delta, _hunk| true),
        Some(&mut |delta, _hunk, line| {
            let file_path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown file".to_string());

            let change_type = match delta.status() {
                Delta::Added => "Added",
                Delta::Deleted => "Deleted",
                Delta::Modified => "Modified",
                Delta::Renamed => "Renamed",
                Delta::Copied => "Copied",
                _ => "Modified",
            }
            .to_string();

            let summary = summarize_change(&line);

            let file_change = changes_map.entry(file_path.clone()).or_insert(FileChange {
                file_path,
                change_type,
                summaries: Vec::new(),
            });

            if !summary.is_empty() {
                file_change.summaries.push(summary);
            }

            true
        }),
    )
    .unwrap();

    changes_map.into_iter().map(|(_, v)| v).collect()
}

fn summarize_change(line: &DiffLine) -> String {
    let content = String::from_utf8_lossy(line.content()).trim().to_string();

    // Limit the length of the content to prevent excessively long summaries
    let truncated_content = if content.len() > 80 {
        format!("{}...", &content[..77])
    } else {
        content.clone()
    };

    match line.origin() {
        '+' => format!("Added: {}", truncated_content),
        '-' => format!("Removed: {}", truncated_content),
        _ => String::new(),
    }
}

fn format_changes_for_prompt(changes: &[FileChange]) -> String {
    let mut formatted = String::new();

    for change in changes {
        formatted.push_str(&format!(
            "- **{}**: {}\n",
            change.file_path, change.change_type
        ));
        for summary in &change.summaries {
            formatted.push_str(&format!("  - {}\n", summary));
        }
    }

    formatted
}

fn is_initial_commit(repo: &Repository) -> bool {
    match repo.head() {
        Ok(_) => false,
        Err(e) => {
            if e.code() == ErrorCode::UnbornBranch {
                true
            } else {
                eprintln!("Error checking HEAD: {}", e);
                false
            }
        }
    }
}
