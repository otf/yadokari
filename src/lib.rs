use std::{collections::HashMap, future::Future, pin::Pin};

use axum::{async_trait, extract::{State, FromRequest}, response::{IntoResponse, Response}, routing::post, Json, Router, BoxError, body::HttpBody, http::Request};
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

#[async_trait]
impl<B> FromRequest<AppState, B> for Event
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request(req: Request<B>, state:&AppState) -> Result<Self, Self::Rejection> {
        let Json(req) = Json::<EventRequest>::from_request(req, state).await
        .map_err(|rejection| {
            let res = Json(json!({
                "error": format!("Json parsing failed, rejection: {}", rejection),
            }));
            (StatusCode::BAD_REQUEST, res)
        })?;

        if req.token != state.verification_token {
            tracing::warn!("Authentication failed, token: {}", req.token);
            let res = Json(json!({
                "ok": false,
            }));
            Err((StatusCode::BAD_REQUEST, res))
        } else {
            req.event.ok_or({
                let res = Json(json!({
                    "ok": true,
                    "challenge": req.challenge
                }));
                (StatusCode::OK, res)
            })
        }
    }
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
                        "text": format!("*<https://www.ur-net.go.jp{}|{}>*\n{}", bukken.bukken_link, bukken.bukken_name, bukken.access.replace("<br>", "\n")),
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
                    {
                        "type": "mrkdwn",
                        "text": format!("*間取り/床面積:*\n{} / {}", bukken.r#type, bukken.floorspace.replace("&#13217;", "㎡")),
                    },
                    {
                        "type": "mrkdwn",
                        "text": format!("*住宅種類:*\n{}", bukken.tokubetsu_kbn_text),
                    },
                ]
            }),
            json!({ "type": "divider" }),
        ])
        .collect::<Vec<_>>();
    json!(bukken_blocks)
}

struct SlackTask(Pin<Box<dyn Future<Output =()> + Send + 'static>>);

impl IntoResponse for SlackTask {
    fn into_response(self: Self) -> Response{
        tokio::spawn(self.0);
        let res = Json(json!({
            "ok": true,
        }));
        (StatusCode::OK, res).into_response()
    }
}

async fn post_events(state: State<AppState>, ev: Event) -> impl IntoResponse {
    SlackTask(Box::pin(async move {
        if ev.user != state.bot_user {
            let bukkens = retrieve_bukken_list().await.unwrap();
            let bukken_blocks = bukkens_to_blocks(&bukkens);
            post_message(&state.bot_user_oauth_token, &ev, &bukken_blocks).await;
        }
    }))
}

async fn post_message(token: &String, ev: &Event, blocks: &Value) {
    let client = reqwest::Client::new();
    let post = PostMessageRequest {
        blocks: blocks.clone(),
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
        bot_user: secret_store.get("BOT_USER").unwrap(),
    };

    let router = Router::new()
        .route("/events", post(post_events))
        .with_state(app_state);

    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
