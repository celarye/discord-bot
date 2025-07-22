use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use twilight_model::user::{CurrentUser, CurrentUserGuild};

use crate::plugins::InitializedPlugins;

#[derive(Serialize)]
pub struct Data {
    pub restart: bool,
    pub current_user: CurrentUser,
    pub current_user_guilds: Vec<CurrentUserGuild>,
    #[serde(with = "arc_rwlock_serde")]
    pub initialized_plugins: Arc<RwLock<InitializedPlugins>>,
}

mod arc_rwlock_serde {
    use std::sync::Arc;

    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};
    use tokio::sync::RwLock;

    pub fn serialize<S, T>(val: &Arc<RwLock<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        T::serialize(&*val.blocking_read(), s)
    }

    pub fn deserialize<'de, D, T>(d: D) -> Result<Arc<RwLock<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Ok(Arc::new(RwLock::new(T::deserialize(d)?)))
    }
}

impl Data {
    pub fn new(
        current_user: CurrentUser,
        current_user_guilds: Vec<CurrentUserGuild>,
        initialized_plugins: Arc<RwLock<InitializedPlugins>>,
    ) -> Box<Self> {
        Box::new(Data {
            restart: false,
            current_user,
            current_user_guilds,
            initialized_plugins,
        })
    }
}
