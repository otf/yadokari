use std::{collections::HashMap};

use axum::{async_trait, extract::{State, FromRequest}, response::{IntoResponse, Response}, routing::post, Json, Router, BoxError, body::HttpBody, http::Request};
use futures::{future::BoxFuture, FutureExt};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use sync_wrapper::SyncWrapper;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    verification_token: String,
    bot_user_oauth_token: String,
    bot_user: String,
    tdfk: String,
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
    rowspan: i32,
    shikutyoson_name: String,
}

async fn retrieve_bukken_list(tdfk: &str) -> reqwest::Result<Vec<Bukken>> {
    let client = reqwest::Client::new();
    let mut params = HashMap::new();
    params.insert("tdfk", tdfk);
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

fn bukkens_to_blocks(bukkens: Vec<&Bukken>) -> Option<Value> {
    if bukkens.len() == 0 {
        return None
    } 

    let bukken_blocks = bukkens
        .iter()
        .take(10) // 50ブロックしかSlackに投稿できないので物件数を制限する。
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

    let yadokari_blocks = vec![
        json!({
            "type": "section",
            "text": {
                "type": "plain_text",
                "text": "新しい特別募集住宅を見つけたカリ:eyes:",
                "emoji": true
            }
        }),
    ];
    Some(json!(itertools::concat(vec![yadokari_blocks, bukken_blocks])))
}

struct SlackTask(BoxFuture<'static, ()>);

impl IntoResponse for SlackTask {
    fn into_response(self: Self) -> Response{
        tokio::spawn(self.0);
        let res = Json(json!({
            "ok": true,
        }));
        (StatusCode::OK, res).into_response()
    }
}

async fn refresh_bukkens(conn: &mut sqlx::PgConnection, bukkens: &Vec<Bukken>) -> sqlx::Result<()> {
    sqlx::query!(r#"TRUNCATE TABLE bukkens"#) 
        .execute(&mut *conn)
        .await?;
    for bukken in bukkens {
        sqlx::query!("INSERT INTO bukkens VALUES ($1, $2, $3)", 
            bukken.bukken_id, bukken.rent_normal, bukken.rowspan
            )
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn filter_fresh<'a>(conn: &mut sqlx::PgConnection, bukkens: &'a Vec<Bukken>) -> sqlx::Result<Vec<&'a Bukken>> {
    let mut fresh_bukkens = Vec::new();

    for bukken in bukkens {
        let count = sqlx::query_scalar!("
            SELECT COUNT(*) 
            FROM bukkens 
            WHERE (bukken_id, rent_normal, rowspan) = ($1, $2, $3)
        ", 
        bukken.bukken_id,
        bukken.rent_normal,
        bukken.rowspan,
        )
            .fetch_one(&mut *conn)
            .await?;
        
        if count == Some(0) {
            fresh_bukkens.push(bukken);
        }
    }
    Ok(fresh_bukkens)
}

async fn post_events(state: State<AppState>, ev: Event) -> impl IntoResponse {
    SlackTask(async move {
        if ev.user != state.bot_user {
            let mut conn = state.pool.acquire().await.unwrap();
            let bukkens = retrieve_bukken_list(&state.tdfk).await.unwrap();
            let fresh_bukkens = filter_fresh(&mut conn, &bukkens).await.unwrap();
            refresh_bukkens(&mut conn, &bukkens).await.unwrap();
            if let Some(bukken_blocks) = bukkens_to_blocks(fresh_bukkens) {
                post_message(&state.bot_user_oauth_token, &ev, &bukken_blocks).await;
            }
        }
    }.boxed())
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
    tracing::info!("{:#?}", res);
}

#[shuttle_service::main]
async fn axum(
    #[shuttle_shared_db::Postgres(
        local_uri = "{secrets.DATABASE_URL}"
    )] pool: PgPool,
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_service::ShuttleAxum {
    std::env::set_var("DATABASE_URL", secret_store.get("DATABASE_URL").unwrap());
    sqlx::migrate!().run(&pool).await.unwrap();

    let app_state = AppState {
        pool,
        verification_token: secret_store.get("VERIFICATION_TOKEN").unwrap(),
        bot_user_oauth_token: secret_store.get("BOT_USER_OAUTH_TOKEN").unwrap(),
        bot_user: secret_store.get("BOT_USER").unwrap(),
        tdfk: secret_store.get("TDFK").unwrap(),
    };

    let router = Router::new()
        .route("/events", post(post_events))
        .with_state(app_state);

    let sync_wrapper = SyncWrapper::new(router);

    Ok(sync_wrapper)
}
