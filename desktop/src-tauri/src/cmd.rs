// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use anyhow::{Context, Result};
use serde::Deserialize;
use tauri::command;
use tauri::State;
use terraphim_pipeline::IndexedDocument;
use terraphim_types::{merge_and_serialize, Article, ConfigState, SearchQuery};

use terraphim_middleware::search_haystacks;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RequestBody {
    id: i32,
    name: String,
}

#[derive(Debug, serde::Serialize)]
pub enum TerraphimTauriError {
    FooError,
}

#[command]
pub fn log_operation(event: String, payload: Option<String>) {
    println!("{} {:?}", event, payload);
}

#[command]
pub fn perform_request(endpoint: String, body: RequestBody) -> String {
    println!("{} {:?}", endpoint, body);
    "message response".into()
}

#[command]
pub async fn my_custom_command(value: &str) -> Result<String, ()> {
    // Call another async function and wait for it to finish
    // some_async_function().await;
    // Note that the return value must be wrapped in `Ok()` now.
    println!("my_custom_command called with {}", value);

    Ok(format!("{}", value))
}

#[command]
pub async fn search(
    config_state: State<'_, ConfigState>,
    search_query: SearchQuery,
) -> Result<Vec<Article>, TerraphimTauriError> {
    println!("Search called with {:?}", search_query);
    let current_config_state = config_state.inner().clone();
    let articles_cached=search_haystacks(current_config_state.clone(), search_query.clone())
        .await
        .context("Failed to search articles")
        .unwrap();
    let docs: Vec<IndexedDocument> = config_state
        .search_articles(search_query)
        .await
        .expect("Failed to search articles");
    let articles = merge_and_serialize(articles_cached, docs).unwrap();

    Ok(articles)
}

#[command]
pub async fn get_config(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<terraphim_config::TerraphimConfig, ()> {
    println!("Get config called");
    let current_config = config_state.config.lock().await;
    println!("Get config called with {:?}", current_config);
    let response = current_config.clone();
    Ok::<terraphim_config::TerraphimConfig, ()>(response)
}

pub struct Port(u16);
/// A command to get the usused port, instead of 3000.
#[tauri::command]
pub fn get_port(port: tauri::State<Port>) -> Result<String, String> {
    Ok(format!("{}", port.0))
}

use std::net::SocketAddr;
use terraphim_server::axum_server;

#[tauri::command]
async fn start_server() -> Result<(), String> {
    let port = portpicker::pick_unused_port().expect("failed to find unused port");
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let config_state = ConfigState::new().await.unwrap();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum_server(addr, config_state).await {
            println!("Failed to start axum server: {e:?}");
        }
    });
    Ok(())
}
