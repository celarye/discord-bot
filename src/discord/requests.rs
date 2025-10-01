use crate::{
    discord::DiscordBotClientSender,
    plugins::runtime::discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
};

impl DiscordBotClientSender {
    pub async fn request(&self, request: DiscordRequests) -> Result<DiscordResponses, String> {
        match request {
            _ => (),
        }

        Ok(vec![])
    }
}
