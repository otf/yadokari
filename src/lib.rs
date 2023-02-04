use axum::{response::IntoResponse, routing::get, Router};
use dotenv::dotenv;
use reqwest::StatusCode;
use serde::Serialize;
use std::env;
use sync_wrapper::SyncWrapper;

#[derive(Serialize)]
struct PostMessageRequest {
    text: String,
    channel: String,
}

async fn post() -> impl IntoResponse {
    dotenv().unwrap();
    let token = env::var("SLACK_BOT_TOKEN").unwrap();
    let client = reqwest::Client::new();
    let post = PostMessageRequest {
        text: "hi".to_owned(),
        channel: "#bot-test".to_owned(),
    };

    let res = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(&token)
        .json(&post)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    (StatusCode::OK, res)
}

#[shuttle_service::main]
async fn axum() -> shuttle_service::ShuttleAxum {
    let router = Router::new().route("/post", get(post));
    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
