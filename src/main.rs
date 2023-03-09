use axum::Server;
use dotenv::dotenv;
use sqlx::PgPool;
use std::{env, net::SocketAddr};
use yadokari::router;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let port = env::var("PORT").expect("PORT must be set").parse().unwrap();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let verification_token = env::var("VERIFICATION_TOKEN")
        .expect("VERIFICATION_TOKEN must be set")
        .parse()
        .unwrap();
    let bot_user_oauth_token = env::var("BOT_USER_OAUTH_TOKEN")
        .expect("BOT_USER_OAUTH_TOKEN must be set")
        .parse()
        .unwrap();
    let bot_user = env::var("BOT_USER")
        .expect("BOT_USER must be set")
        .parse()
        .unwrap();
    let tdfk = env::var("TDFK").expect("TDFK must be set").parse().unwrap();
    let pool = PgPool::connect(&db_url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();

    let router = router(
        verification_token,
        bot_user_oauth_token,
        bot_user,
        tdfk,
        pool,
    );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    Server::bind(&addr)
        .serve(router.into_make_service())
        .await
        .unwrap();
}
