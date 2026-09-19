use crate::announcements::AnnouncementStatus;
use serenity::{
    all::{Channel, ChannelId, ChannelType, GuildId, Permissions, UserId},
    http::Http,
};

const CHANNEL_ERROR: &str = "Choose a text channel in this server.";
const READ_ERROR: &str = "I could not verify that channel. Please try again.";
const PERMISSION_ERROR: &str = "I need View Channel and Send Messages in that channel.";

/// Verify that a destination is an ordinary text channel in the invoking guild and that the bot
/// can see and send messages there.
pub(super) async fn validate_destination(
    http: &Http,
    guild: u64,
    bot_user_id: u64,
    channel_id: u64,
) -> Result<(), &'static str> {
    if guild == 0 || bot_user_id == 0 || channel_id == 0 {
        return Err(CHANNEL_ERROR);
    }

    let channel = ChannelId::new(channel_id)
        .to_channel(http)
        .await
        .map_err(|_| READ_ERROR)?;
    let Channel::Guild(channel) = channel else {
        return Err(CHANNEL_ERROR);
    };
    if channel.kind != ChannelType::Text || channel.guild_id.get() != guild {
        return Err(CHANNEL_ERROR);
    }

    let guild_id = GuildId::new(guild);
    let partial = guild_id
        .to_partial_guild(http)
        .await
        .map_err(|_| READ_ERROR)?;
    let member = guild_id
        .member(http, UserId::new(bot_user_id))
        .await
        .map_err(|_| READ_ERROR)?;
    let permissions = partial.user_permissions_in(&channel, &member);
    if !permissions.contains(Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES) {
        return Err(PERMISSION_ERROR);
    }
    Ok(())
}

pub(super) fn configuration_receipt(message: &str, disabled: bool) -> String {
    if disabled {
        format!(
            "{message} Pending announcements were discarded. An announcement already in flight may still reach the previous channel."
        )
    } else {
        format!(
            "{message} Pending announcements will use this destination. An announcement already in flight may still reach the previous channel."
        )
    }
}

pub(super) fn render_status(status: &AnnouncementStatus) -> String {
    let channel = status
        .channel_id
        .map_or_else(|| "not configured".to_owned(), |id| format!("<#{id}>"));
    let state = if !status.enabled {
        "disabled"
    } else if status.pause_reason.is_some() {
        "paused"
    } else {
        "enabled"
    };
    let reason = status
        .pause_reason
        .as_deref()
        .map_or_else(|| "none".to_owned(), str::to_owned);
    format!(
        "Announcement channel: {channel}
Status: {state}
Pending: {}
Reason: {reason}",
        status.pending
    )
}
