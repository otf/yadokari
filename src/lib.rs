use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dotenv::dotenv;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;
use sync_wrapper::SyncWrapper;

#[derive(Serialize)]
struct PostMessageRequest {
    text: String,
    channel: String,
}

#[derive(Deserialize)]
struct ChallengeRequest {
    token: String,
    challenge: Option<String>,
}

#[derive(Serialize)]
struct ChallengeResponse {
    ok: bool,
    challenge: Option<String>,
}

async fn challenge(Json(req): Json<ChallengeRequest>) -> impl IntoResponse {
    let verification_token = env::var("VERIFICATION_TOKEN").unwrap();

    if req.token != verification_token {
        tracing::warn!("AuthenticationFailed, token {}", req.token);
        let res = Json(ChallengeResponse {
            ok: false,
            challenge: None,
        });
        (StatusCode::BAD_REQUEST, res)
    } else {
        let res = Json(ChallengeResponse {
            ok: true,
            challenge: req.challenge,
        });
        (StatusCode::OK, res)
    }
}

async fn post_message() -> impl IntoResponse {
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
    let router = Router::new()
        .route("/challenge", post(challenge))
        .route("/post", get(post_message));
    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
