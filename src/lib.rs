use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dotenv::dotenv;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use shuttle_secrets::SecretStore;
use std::env;
use sync_wrapper::SyncWrapper;

#[derive(Clone)]
struct AppState {
    verification_token: String,
}

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

async fn challenge(state: State<AppState>, Json(req): Json<ChallengeRequest>) -> impl IntoResponse {
    if req.token != state.verification_token {
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
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_service::ShuttleAxum {
    let app_state = AppState {
        verification_token: secret_store.get("VERIFICATION_TOKEN").unwrap(),
    };

    let router = Router::new()
        .route("/challenge", post(challenge))
        .route("/post", get(post_message))
        .with_state(app_state);

    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
