//! Logseq indexer
//!
//! Logseq is a knowledge graph that uses Markdown files to store notes. This
//! module provides a middleware for indexing and searching through Logseq
//! haystacks.
//!
//! At the moment, we only support searching for synonyms in Logseq haystacks.
//! Example:
//!
//! ```
//! synonyms:: foo, bar, bazfoo
//! ```

use cached::proc_macro::cached;
use std::collections::HashSet;
use std::fs::{self};
use std::path::Path;
use std::process::Stdio;
use terraphim_types::{Article, Index};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::{calculate_hash, IndexMiddleware};
use crate::command::ripgrep::{json_decode, Data, Message};
use crate::Result;

/// In Logseq, `::` serves as a delimiter between the property name and its
/// value, e.g.
///
/// ```
/// title:: My Note
/// tags:: #idea #project
/// ```
const LOGSEQ_KEY_VALUE_DELIMITER: &str = "::";

/// LogseqMiddleware is a Middleware that uses ripgrep to index and search
/// through haystacks.
pub struct LogseqIndexer {
    service: LogseqService,
}

impl Default for LogseqIndexer {
    fn default() -> Self {
        Self {
            service: LogseqService::default(),
        }
    }
}

impl IndexMiddleware for LogseqIndexer {
    /// Index the haystack using ripgrep and return a HashMap of Articles
    ///
    /// # Errors
    ///
    /// Returns an error if the middleware fails to index the haystack
    async fn index(&self, needle: &str, haystack: &Path) -> Result<Index> {
        let messages = self
            .service
            .get_raw_messages(LOGSEQ_KEY_VALUE_DELIMITER, haystack)
            .await?;

        let articles = index_inner(messages);
        Ok(articles)
    }
}

pub struct LogseqService {
    command: String,
    default_args: Vec<String>,
}

/// Returns a new ripgrep service with default arguments
impl Default for LogseqService {
    fn default() -> Self {
        Self {
            command: "rg".to_string(),
            default_args: ["--json", "--trim", "--ignore-case"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

impl LogseqService {
    /// Run ripgrep with the given needle and haystack
    ///
    /// Returns a Vec of Messages, which correspond to ripgrep's internal
    /// JSON output. Learn more about ripgrep's JSON output here:
    /// https://docs.rs/grep-printer/0.2.1/grep_printer/struct.JSON.html
    pub async fn get_raw_messages(&self, needle: &str, haystack: &Path) -> Result<Vec<Message>> {
        let haystack = haystack.to_string_lossy().to_string();
        println!("Running logseq with needle: {needle} and haystack: {haystack}");

        // Merge the default arguments with the needle and haystack
        let args: Vec<String> = vec![needle.to_string(), haystack]
            .into_iter()
            .chain(self.default_args.clone())
            .collect();

        let mut child = Command::new(&self.command)
            .args(args)
            .stdout(Stdio::piped())
            .spawn()?;

        let mut stdout = child.stdout.take().expect("Stdout is not available");
        let read = async move {
            let mut data = String::new();
            stdout.read_to_string(&mut data).await.map(|_| data)
        };
        let output = read.await?;
        json_decode(&output)
    }
}

#[cached]
/// Indexes the articles from raw ripgrep messages
///
/// This is a free-standing function because it's a requirement for caching the
/// results
fn index_inner(messages: Vec<Message>) -> Index {
    // Cache of the articles already processed by index service
    let mut cached_articles = Index::new();
    let mut existing_paths: HashSet<String> = HashSet::new();

    let mut article = Article::default();
    for message in messages {
        match message {
            Message::Begin(message) => {
                article = Article::default();

                let Some(path) = message.path() else {
                    continue;
                };
                if existing_paths.contains(&path) {
                    continue;
                }
                existing_paths.insert(path.clone());

                let id = calculate_hash(&path);

                article.id = Some(id.clone());
                article.title = path.clone();
                article.url = path.clone();
            }
            Message::Match(message) => {
                let Some(path) = message.path() else {
                    continue;
                };

                let body = match fs::read_to_string(path) {
                    Ok(body) => body,
                    Err(e) => {
                        println!("Error: Failed to read file: {:?}. Skipping", e);
                        continue;
                    }
                };
                article.body = body;

                let lines = match &message.lines {
                    Data::Text { text } => text,
                    _ => {
                        println!("Error: lines is not text: {:?}", message.lines);
                        continue;
                    }
                };
                match article.description {
                    Some(description) => {
                        article.description = Some(description + " " + &lines);
                    }
                    None => {
                        article.description = Some(lines.clone());
                    }
                }
            }
            Message::Context(message) => {
                let Some(path) = message.path() else {
                    continue;
                };
                let article_url = article.url.clone();

                // We got a context for a different article
                if article_url != *path {
                    println!(
                            "Error: Context for differrent article. article_url != path_text: {article_url:?} != {path:?}"
                        );
                    continue;
                }

                let lines = match &message.lines {
                    Data::Text { text } => text,
                    _ => {
                        println!("Error: lines is not text: {:?}", message.lines);
                        continue;
                    }
                };
                match article.description {
                    Some(description) => {
                        article.description = Some(description + " " + &lines);
                    }
                    None => {
                        article.description = Some(lines.clone());
                    }
                }
            }
            Message::End(_) => {
                // The `End` message could be received before the `Begin`
                // message causing the article to be empty
                let id = match article.id {
                    Some(ref id) => id,
                    None => {
                        println!("Error: End message received before Begin message. Skipping.");
                        continue;
                    }
                };
                // We are done with the article. Add it to the cache.
                cached_articles.insert(id.to_string(), article.clone());
            }
            _ => {}
        };
    }

    cached_articles
}
