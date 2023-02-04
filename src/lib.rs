use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use shuttle_secrets::SecretStore;
use sync_wrapper::SyncWrapper;

#[derive(Clone)]
struct AppState {
    verification_token: String,
    bot_user_oauth_token: String,
}

#[derive(Serialize)]
struct PostMessageRequest {
    text: String,
    channel: String,
}

#[derive(Deserialize)]
struct Event {
    channel: String,
    user: String,
    text: String,
}

#[derive(Deserialize)]
struct EventRequest {
    token: String,
    challenge: Option<String>,
    event: Option<Event>,
}

#[derive(Serialize)]
struct EventResponse {
    ok: bool,
    challenge: Option<String>,
}

async fn post_events(state: State<AppState>, Json(req): Json<EventRequest>) -> impl IntoResponse {
    if req.token != state.verification_token {
        tracing::warn!("AuthenticationFailed, token {}", req.token);
        let res = Json(EventResponse {
            ok: false,
            challenge: None,
        });
        (StatusCode::BAD_REQUEST, res)
    } else {
        match &req.event {
            Some(ev) => {
                if ev.user != "yadokari" {
                    post_echo(&state.bot_user_oauth_token, ev).await;
                }
            }
            None => {}
        }
        let res = Json(EventResponse {
            ok: true,
            challenge: req.challenge,
        });
        (StatusCode::OK, res)
    }
}

async fn post_echo(token: &String, ev: &Event) {
    let client = reqwest::Client::new();
    let post = PostMessageRequest {
        text: ev.text.clone(),
        channel: ev.channel.clone(),
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
}

#[shuttle_service::main]
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_service::ShuttleAxum {
    let app_state = AppState {
        verification_token: secret_store.get("VERIFICATION_TOKEN").unwrap(),
        bot_user_oauth_token: secret_store.get("BOT_USER_OAUTH_TOKEN").unwrap(),
    };

    let router = Router::new()
        .route("/events", post(post_events))
        .with_state(app_state);

    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
