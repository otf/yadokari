use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
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
    text: String,
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
    rowspan: String,
    shikutyoson_name: String,
}

async fn get_bukken_list() -> Vec<Bukken> {
    let client = reqwest::Client::new();
    client
        .post("https://chintai.sumai.ur-net.go.jp/chintai/api/tokubetsu/list_tokubetsu")
        .form(&[("tdfk", "23", ("is_sp", false))])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
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
                    tracing::info!("{:#?}", ev);
                    let bukkens = get_bukken_list().await;
                    tracing::info!("{:#?}", bukkens);
                    post_message(
                        &state.bot_user_oauth_token,
                        ev,
                        serde_json::to_string(&bukkens).unwrap().as_str(),
                    )
                    .await;
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

async fn post_message(token: &String, ev: &Event, text: &str) {
    let client = reqwest::Client::new();
    let post = PostMessageRequest {
        text: text.to_owned(),
        channel: ev.channel.clone(),
    };

    let _res = client
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
