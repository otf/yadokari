use std::collections::HashMap;

use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shuttle_secrets::SecretStore;
use sync_wrapper::SyncWrapper;

#[derive(Clone)]
struct AppState {
    verification_token: String,
    bot_user_oauth_token: String,
    bot_user: String,
}

#[derive(Serialize)]
struct PostMessageRequest {
    blocks: Value,
    channel: String,
}

#[derive(Deserialize, Debug)]
struct Event {
    channel: String,
    user: String,
    #[allow(dead_code)]
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

#[derive(Deserialize, Serialize, Debug)]
#[allow(dead_code)]
struct Bukken {
    bukken_id: String,
    bukken_name: String,
    bukken_link: String,
    image: String,
    rent_normal: String,
    rent_waribiki: String,
    commonfee_normal: String,
    commonfee_waribiki: String,
    span: String,
    r#type: String,
    floorspace: String,
    floor: String,
    floor_max: String,
    access: String,
    tokubetsu_kbn_text: String,
    tokubetsu_kbn: String,
    shikutyoson_name: String,
}

async fn retrieve_bukken_list() -> reqwest::Result<Vec<Bukken>> {
    let client = reqwest::Client::new();
    let mut params = HashMap::new();
    params.insert("tdfk", "13"); // 東京
    params.insert("is_sp", "false");
    client
        .post("https://chintai.sumai.ur-net.go.jp/chintai/api/tokubetsu/list_tokubetsu")
        .form(&params)
        .send()
        .await?
        .json()
        .await
}

fn bukkens_to_blocks(bukkens: &Vec<Bukken>) -> Value {
    let bukken_blocks = bukkens
        .into_iter()
        .take(3) // todo: 50ブロックになるように分けて投稿する。
        .flat_map(|bukken| 
            [ json!({ "type": "divider" }),
            json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("*<https://www.ur-net.go.jp{}|{}>*", bukken.bukken_link, bukken.bukken_name),
                    },
                    "accessory": {
                        "type": "image",
                        "image_url": bukken.image,
                        "alt_text": bukken.bukken_name,
                    }
            }),
            json!({
                "type": "section",
                "fields": [
                    {
                        "type": "mrkdwn",
                        "text": format!("*通常家賃(共益費):*\n{}{}", bukken.rent_normal, bukken.commonfee_normal),
                    },
                    {
                        "type": "mrkdwn",
                        "text": format!("*割引後家賃(共益費):*\n{}{}", bukken.rent_waribiki, bukken.commonfee_waribiki),
                    },
                ]
            }),
            json!({ "type": "divider" }),
        ])
        .collect::<Vec<_>>();
    json!(bukken_blocks)
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
                if ev.user != state.bot_user {
                    let bukkens = retrieve_bukken_list().await.unwrap();
                    let bukken_blocks = bukkens_to_blocks(&bukkens);
                    post_message(&state.bot_user_oauth_token, ev, &bukken_blocks).await.unwrap();
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

async fn post_message(token: &String, ev: &Event, blocks: &Value) -> reqwest::Result<String> {
    let client = reqwest::Client::new();
    let post = PostMessageRequest {
        blocks: blocks.clone(),
        channel: ev.channel.clone(),
    };

    client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(&token)
        .json(&post)
        .send()
        .await?
        .text()
        .await
}

#[shuttle_service::main]
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_service::ShuttleAxum {
    let app_state = AppState {
        verification_token: secret_store.get("VERIFICATION_TOKEN").unwrap(),
        bot_user_oauth_token: secret_store.get("BOT_USER_OAUTH_TOKEN").unwrap(),
        bot_user: secret_store.get("BOT_USER").unwrap(),
    };

    let router = Router::new()
        .route("/events", post(post_events))
        .with_state(app_state);

    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
