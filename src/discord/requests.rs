use tracing::error;
use twilight_http::{request::Request, routing::Route};
use twilight_model::{
    gateway::{
        OpCode,
        payload::outgoing::{
            RequestGuildMembers, UpdatePresence, UpdateVoiceState,
            request_guild_members::RequestGuildMembersInfo, update_presence::UpdatePresencePayload,
            update_voice_state::UpdateVoiceStateInfo,
        },
    },
    id::Id,
};

use crate::{
    discord::DiscordBotClient,
    plugins::discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
};

impl DiscordBotClient {
    pub async fn request(
        &self,
        request: DiscordRequests,
    ) -> Result<Option<DiscordResponses>, String> {
        let request = match request {
            DiscordRequests::RequestGuildMembers((guild_id, mut body)) => {
                let guild_id = Id::new(guild_id);

                let guild_shard_message_sender = if let Some(guild_shard_message_sender) =
                    self.shard_message_senders.read().await.get(&guild_id)
                {
                    guild_shard_message_sender.clone()
                } else {
                    let err = String::from("No guild found");
                    error!(err);
                    return Err(err);
                };

                let d = match simd_json::from_slice::<RequestGuildMembersInfo>(&mut body) {
                    Ok(d) => d,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while deserializing RequestGuildMembersInfo, error: {err}",
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                let request_guild_members = RequestGuildMembers {
                    d,
                    op: OpCode::RequestGuildMembers,
                };

                let _ = guild_shard_message_sender.command(&request_guild_members);

                None
            }
            DiscordRequests::RequestSoundboardSounds(_guild_ids) => {
                let err = String::from(
                    "RequestSoundboardSounds has not yet been implemented in Twilight.",
                );
                error!(err);
                return Err(err);
            }
            DiscordRequests::UpdateVoiceState((guild_id, mut body)) => {
                let guild_id = Id::new(guild_id);

                let guild_shard_message_sender = if let Some(guild_shard_message_sender) =
                    self.shard_message_senders.read().await.get(&guild_id)
                {
                    guild_shard_message_sender.clone()
                } else {
                    let err = String::from("No guild found");
                    error!(err);
                    return Err(err);
                };

                let d = match simd_json::from_slice::<UpdateVoiceStateInfo>(&mut body) {
                    Ok(d) => d,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while deserializing RequestGuildMembersInfo, error: {err}",
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                let update_voice_state = UpdateVoiceState {
                    d,
                    op: OpCode::RequestGuildMembers,
                };

                let _ = guild_shard_message_sender.command(&update_voice_state);

                None
            }
            DiscordRequests::UpdatePresence(mut body) => {
                let guild_shard_message_sender = if let Some(guild_shard_message_sender) =
                    self.shard_message_senders.read().await.values().next()
                {
                    guild_shard_message_sender.clone()
                } else {
                    let err = String::from("No guild found");
                    error!(err);
                    return Err(err);
                };

                let d = match simd_json::from_slice::<UpdatePresencePayload>(&mut body) {
                    Ok(d) => d,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while deserializing RequestGuildMembersInfo, error: {err}",
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                let update_voice_state = UpdatePresence {
                    d,
                    op: OpCode::RequestGuildMembers,
                };

                let _ = guild_shard_message_sender.command(&update_voice_state);

                None
            }

            DiscordRequests::AddThreadMember((channel_id, user_id)) => {
                match Request::builder(&Route::AddThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::CreateForumThread((channel_id, body)) => {
                match Request::builder(&Route::CreateForumThread { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::CreateMessage((channel_id, body)) => {
                match Request::builder(&Route::CreateMessage { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::CreateThread((channel_id, body)) => {
                match Request::builder(&Route::CreateThread { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::CreateThreadFromMessage((channel_id, message_id, body)) => {
                match Request::builder(&Route::CreateThreadFromMessage {
                    channel_id,
                    message_id,
                })
                .body(body)
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetActiveThreads(guild_id) => {
                match Request::builder(&Route::GetActiveThreads { guild_id }).build() {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetChannel(channel_id) => {
                match Request::builder(&Route::GetChannel { channel_id }).build() {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetJoinedPrivateArchivedThreads((before, channel_id, limit)) => {
                match Request::builder(&Route::GetJoinedPrivateArchivedThreads {
                    before,
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetPrivateArchivedThreads((before, channel_id, limit)) => {
                match Request::builder(&Route::GetPrivateArchivedThreads {
                    before: before.as_deref(),
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetPublicArchivedThreads((before, channel_id, limit)) => {
                match Request::builder(&Route::GetPublicArchivedThreads {
                    before: before.as_deref(),
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetThreadMember((channel_id, user_id)) => {
                match Request::builder(&Route::GetThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::GetThreadMembers((after, channel_id, limit, with_member)) => {
                match Request::builder(&Route::GetThreadMembers {
                    after,
                    channel_id,
                    limit,
                    with_member,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::InteractionCallback((interaction_id, interaction_token, body)) => {
                match Request::builder(&Route::InteractionCallback {
                    interaction_id,
                    interaction_token: &interaction_token,
                })
                .body(body)
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::JoinThread(channel_id) => {
                match Request::builder(&Route::JoinThread { channel_id }).build() {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::LeaveThread(channel_id) => {
                match Request::builder(&Route::LeaveThread { channel_id }).build() {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
            DiscordRequests::RemoveThreadMember((channel_id, user_id)) => {
                match Request::builder(&Route::RemoveThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => Some(request),
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                }
            }
        };

        if let Some(request) = request {
            match self.http_client.request::<Vec<u8>>(request).await {
                Ok(response) => Ok(Some(response.bytes().await.unwrap().clone())),
                Err(err) => {
                    let err = format!(
                        "Something went wrong while making a Discord request, error: {err}"
                    );
                    error!(err);
                    Err(err)
                }
            }
        } else {
            Ok(None)
        }
    }
}
