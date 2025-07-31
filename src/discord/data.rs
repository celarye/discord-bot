use serde::Serialize;
use tokio::sync::RwLock;
use twilight_model::user::{CurrentUser, CurrentUserGuild};

use crate::plugins::InitializedPlugins;

#[derive(Serialize)]
pub struct Data {
    #[serde(with = "rwlock_serde")]
    pub restart: RwLock<bool>,
    #[serde(with = "rwlock_serde")]
    pub current_user: RwLock<CurrentUser>,
    #[serde(with = "rwlock_serde")]
    pub current_user_guilds: RwLock<Vec<CurrentUserGuild>>,
    #[serde(with = "rwlock_serde")]
    pub initialized_plugins: RwLock<InitializedPlugins>,
}

mod rwlock_serde {
    use serde::Serialize;
    use serde::ser::Serializer;
    use tokio::sync::RwLock;

    pub fn serialize<S, T>(val: &RwLock<T>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        T::serialize(&*val.blocking_read(), s)
    }
}

impl Data {
    pub fn new(
        current_user: CurrentUser,
        current_user_guilds: Vec<CurrentUserGuild>,
        initialized_plugins: InitializedPlugins,
    ) -> Self {
        Data {
            restart: RwLock::new(false),
            current_user: RwLock::new(current_user),
            current_user_guilds: RwLock::new(current_user_guilds),
            initialized_plugins: RwLock::new(initialized_plugins),
        }
    }
}
