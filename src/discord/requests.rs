use tracing::error;
use twilight_http::{
    request::{
        Request,
        channel::{
            GetChannel,
            message::CreateMessage,
            thread::{
                AddThreadMember, CreateForumThread, CreateThread, CreateThreadFromMessage,
                GetJoinedPrivateArchivedThreads, GetPrivateArchivedThreads,
                GetPublicArchivedThreads, GetThreadMember, GetThreadMembers, JoinThread,
                LeaveThread, RemoveThreadMember,
            },
        },
        guild::GetActiveThreads,
    },
    routing::Route,
};
use twilight_model::{
    gateway::{
        OpCode,
        payload::outgoing::{
            RequestGuildMembers, UpdatePresence, UpdateVoiceState,
            request_guild_members::RequestGuildMembersInfo, update_presence::UpdatePresencePayload,
            update_voice_state::UpdateVoiceStateInfo,
        },
    },
    http::interaction::InteractionResponse,
    id::Id,
};

use crate::{
    discord::DiscordBotClient,
    plugins::discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
};

impl DiscordBotClient {
    pub async fn request(&self, request: DiscordRequests) -> Result<DiscordResponses, String> {
        match request {
            DiscordRequests::RequestGuildMembers((guild_id, mut body)) => {
                let guild_id = Id::new(guild_id);

                let guild_shard_message_sender =
                    match self.shard_message_senders.read().await.get(&guild_id) {
                        Some(guild_shard_message_sender) => guild_shard_message_sender.clone(),
                        None => {
                            let err = String::from("No guild found");
                            error!(err);
                            return Err(err);
                        }
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

                Ok(vec![])
            }
            DiscordRequests::RequestSoundboardSounds(_guild_ids) => {
                let err = String::from(
                    "RequestSoundboardSounds has not yet been implemented in Twilight.",
                );
                error!(err);
                Err(err)
            }
            DiscordRequests::UpdateVoiceState((guild_id, mut body)) => {
                let guild_id = Id::new(guild_id);

                let guild_shard_message_sender =
                    match self.shard_message_senders.read().await.get(&guild_id) {
                        Some(guild_shard_message_sender) => guild_shard_message_sender.clone(),
                        None => {
                            let err = String::from("No guild found");
                            error!(err);
                            return Err(err);
                        }
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

                Ok(vec![])
            }
            DiscordRequests::UpdatePresence(mut body) => {
                let guild_shard_message_sender =
                    match self.shard_message_senders.read().await.values().next() {
                        Some(guild_shard_message_sender) => guild_shard_message_sender.clone(),
                        None => {
                            let err = String::from("No guild found");
                            error!(err);
                            return Err(err);
                        }
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

                Ok(vec![])
            }

            DiscordRequests::AddThreadMember((channel_id, user_id)) => {
                let request = match Request::builder(&Route::AddThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<AddThreadMember>(request).await
            }
            DiscordRequests::CreateForumThread((channel_id, body)) => {
                let request = match Request::builder(&Route::CreateForumThread { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<CreateForumThread>(request).await
            }
            DiscordRequests::CreateMessage((channel_id, body)) => {
                let request = match Request::builder(&Route::CreateMessage { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<CreateMessage>(request).await
            }
            DiscordRequests::CreateThread((channel_id, body)) => {
                let request = match Request::builder(&Route::CreateThread { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<CreateThread>(request).await
            }
            DiscordRequests::CreateThreadFromMessage((channel_id, message_id, body)) => {
                let request = match Request::builder(&Route::CreateThreadFromMessage {
                    channel_id,
                    message_id,
                })
                .body(body)
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<CreateThreadFromMessage>(request)
                    .await
            }
            DiscordRequests::GetActiveThreads(guild_id) => {
                let request = match Request::builder(&Route::GetActiveThreads { guild_id }).build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetActiveThreads>(request).await
            }
            DiscordRequests::GetChannel(channel_id) => {
                let request = match Request::builder(&Route::GetChannel { channel_id }).build() {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetChannel>(request).await
            }
            DiscordRequests::GetJoinedPrivateArchivedThreads((before, channel_id, limit)) => {
                let request = match Request::builder(&Route::GetJoinedPrivateArchivedThreads {
                    before,
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetJoinedPrivateArchivedThreads>(request)
                    .await
            }
            DiscordRequests::GetPrivateArchivedThreads((before, channel_id, limit)) => {
                let request = match Request::builder(&Route::GetPrivateArchivedThreads {
                    before: before.as_deref(),
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetPrivateArchivedThreads>(request)
                    .await
            }
            DiscordRequests::GetPublicArchivedThreads((before, channel_id, limit)) => {
                let request = match Request::builder(&Route::GetPublicArchivedThreads {
                    before: before.as_deref(),
                    channel_id,
                    limit,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetPublicArchivedThreads>(request)
                    .await
            }
            DiscordRequests::GetThreadMember((channel_id, user_id)) => {
                let request = match Request::builder(&Route::GetThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetThreadMember>(request).await
            }
            DiscordRequests::GetThreadMembers((after, channel_id, limit, with_member)) => {
                let request = match Request::builder(&Route::GetThreadMembers {
                    after,
                    channel_id,
                    limit,
                    with_member,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<GetThreadMembers>(request).await
            }
            DiscordRequests::InteractionCallback((interaction_id, interaction_token, body)) => {
                let request = match Request::builder(&Route::InteractionCallback {
                    interaction_id,
                    interaction_token: &interaction_token,
                })
                .body(body)
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<InteractionResponse>(request).await
            }
            DiscordRequests::JoinThread(channel_id) => {
                let request = match Request::builder(&Route::JoinThread { channel_id }).build() {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<JoinThread>(request).await
            }
            DiscordRequests::LeaveThread(channel_id) => {
                let request = match Request::builder(&Route::LeaveThread { channel_id }).build() {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<LeaveThread>(request).await
            }
            DiscordRequests::RemoveThreadMember((channel_id, user_id)) => {
                let request = match Request::builder(&Route::RemoveThreadMember {
                    channel_id,
                    user_id,
                })
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<RemoveThreadMember>(request).await
            }
        }
    }

    async fn request_execute<T: Unpin>(
        &self,
        request: Request,
    ) -> Result<DiscordResponses, String> {
        match self.http_client.request::<T>(request).await {
            Ok(response) => Ok(response.bytes().await.unwrap().to_vec()),
            Err(err) => {
                let err =
                    format!("Something went wrong while making a Discord request, error: {err}");
                error!(err);
                Err(err)
            }
        }
    }
}
