use anyhow::{Context as _, anyhow};
use axum::{extract::State, http::StatusCode, Router, routing::post};
use axum_extra::TypedHeader;
use std::sync::Arc;
use futures_util::TryStreamExt as _;
use headers::{Authorization, authorization::Bearer};
use twitch_api::eventsub::{
    Conduit, Shard, Transport,
    channel::ChannelChatMessageV1
};
use twitch_api::helix::users::User;
use twitch_api::TwitchClient;
use twitch_api::twitch_oauth2::AppAccessToken;

struct ControlState<'a> {
    client: TwitchClient<'a, reqwest::Client>,
    app_token: AppAccessToken,
    conduit: Conduit,
    token: String
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenvy::dotenv().ok();

    let control_port             = std::env::var("CONTROL_PORT"            ).context("missing CONTROL_PORT")?.parse::<u16>()?;
    let control_hardcoded_token  = std::env::var("CONTROL_HARDCODED_TOKEN" ).context("missing CONTROL_HARDCODED_TOKEN")?;
    let twitch_client_id         = std::env::var("TWITCH_CLIENT_ID"        ).context("missing TWITCH_CLIENT_ID")?;
    let twitch_client_secret     = std::env::var("TWITCH_CLIENT_SECRET"    ).context("missing TWITCH_CLIENT_SECRET")?;
    let twitch_user_login        = std::env::var("TWITCH_USER_LOGIN"       ).context("missing TWITCH_USER_LOGIN")?;
    let twitch_broadcaster_login = std::env::var("TWITCH_BROADCASTER_LOGIN").context("missing TWITCH_BROADCASTER_LOGIN")?;

    let client: TwitchClient<reqwest::Client> = TwitchClient::default();
    let app_token = AppAccessToken::get_app_access_token(
        &client,
        twitch_client_id.into(),
        twitch_client_secret.into(),
        vec![]
    ).await?;

    let conduits = client.helix.get_conduits(&app_token).await?;

    log::info!("{conduits:?}");

    let conduit = if let Some(c) = conduits.into_iter().next() {
        c
    } else {
        client.helix.create_conduit(1, &app_token).await?
    };

    log::info!("{conduit:?}");

    let my_user = client.helix.get_user_from_login(&twitch_user_login, &app_token).await?.ok_or_else(|| anyhow!("failed to retrieve my user"))?;
    let broadcaster_users: Vec<User> = client.helix.get_users_from_logins(&[twitch_broadcaster_login][..].into(), &app_token).try_collect().await?;

    log::info!("{broadcaster_users:?}");

    for broadcaster_user in broadcaster_users {
        match client.helix.create_eventsub_subscription(
            ChannelChatMessageV1::new(broadcaster_user.id, my_user.id.clone()),
            Transport::conduit(&conduit.id),
            &app_token
        ).await {
            Ok(event_info) => {
                log::info!("{event_info:?}");
            },
            Err(e) => {
                log::error!("{e:?}");
            }
        }
    }

    // control server stuff

    let control_state = Arc::new(ControlState {
        client,
        app_token,
        conduit,
        token: control_hardcoded_token
    });

    let app = Router::new()
        .route("/session/assign", post(session_assign))
        .with_state(control_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", control_port)).await?;

    axum::serve(listener, app).await?;

    Ok(())
}

async fn session_assign(
    State(control_state): State<Arc<ControlState<'_>>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    body: String
) -> Result<&'static str, StatusCode> {
    if bearer.token() != control_state.token {
        Err(reqwest::StatusCode::UNAUTHORIZED)
    } else {
        let shard = Shard::new("0", Transport::websocket(body));
        let r = control_state.client.helix.update_conduit_shards(control_state.conduit.id.clone(), &[shard], &control_state.app_token).await;
        log::info!("{r:?}");

        Ok("Hello world!")
    }
}
