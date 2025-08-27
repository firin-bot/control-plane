use anyhow::{Context as _, Result, anyhow};
use futures_util::TryStreamExt as _;
use twitch_api::eventsub::{
    Transport,
    channel::ChannelChatMessageV1
};
use twitch_api::helix::users::User;
use twitch_api::TwitchClient;
use twitch_api::twitch_oauth2::AppAccessToken;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    dotenvy::dotenv().ok();

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
        let event_info = client.helix.create_eventsub_subscription(
            ChannelChatMessageV1::new(broadcaster_user.id, my_user.id.clone()),
            Transport::conduit(&conduit.id),
            &app_token
        ).await?;

        log::info!("{event_info:?}");
    }

    Ok(())
}
