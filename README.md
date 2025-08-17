# Discord Bot

> [!NOTE]
> This project is still in **very early development**. While it is open source,
> it is **not open to contributions** just yet. This will change once version
> `v0.1.0` is reached.
> The current goal is to reach `v0.1.0` sometime in early to mid October.

> [!WARNING]
> The current commit history will **frequently be rewritten** until `v0.1.0` is
> released.

A WASM plugin based Discord bot, configurable through YAML.

## Project Goal

The goal of this project is to create a **Docker Compose-like experience** for
Discord bots.

Users are be able to self-host their own personal bot, assembled from plugins
available from the official registry.

This official registry contains plugins covering the most common features
required from Discord bots.

Programmers are also be able to add their own custom plugins. This allows them
to focus on what really matters (the functionality of that plugin) while
relying on features provided by other plugins.

## To Do List

- [ ] Codebase restructure
- [ ] Complete the job scheduler
- [ ] Implement all WASI host functions
- [ ] Implement the Discord request handler
- [ ] Add support for  other Discord events and requests
- [ ] Make plugins

### Codebase Restructure

- [ ] Data:

```rust
#[derive(Serialize)]
pub struct Data {
    pub current_user: CurrentUser,
    #[serde(with = "rwlock_serde")]
    pub current_user_guilds: RwLock<Vec<CurrentUserGuild>>,
    #[serde(with = "rwlock_serde")]
    pub initialized_plugins: RwLock<InitializedPlugins>,
}
```

- [ ] Plugins:

```rust
#[derive(Serialize)]
pub struct InitializedPlugins {
    pub discord_events: InitializedPluginsDiscordEvents,
    pub scheduled_jobs: HashMap<String, Vec<InitializedPluginsScheduledJob>>,
    pub dependencies: HashMap<String, HashSet<String>>,
}

#[derive(Serialize)]
pub struct InitializedPluginsDiscordEvents {
    pub interaction_create_commands: HashMap<String, InitializedPluginsDiscordEventsInteractionCreateCommand>,
    pub message_create: Vec<String>,
}

pub struct InitializedPluginsDiscordEventsInteractionCreateCommand {
  pub plugin_name: String,
  pub internal_command_name: String,
}

#[derive(Serialize)]
pub struct InitializedPluginsScheduledJob {
  pub plugin: String,
  pub job: String,
}

pub struct InitializedPluginRegistrations {
    pub commands: Vec<InitializedPluginRegistrationsCommand>,
}

pub struct InitializedPluginRegistrationsCommand {
    pub plugin_name: String,
    pub intial_response: Vec<u8>,
    pub command_data: Vec<u8>,
}
```

- [ ] Discord:

```rust
struct DiscordBotClientReceiver {
  shards: Box<dyn ExactSizeIterator<Item = Shard>>,
  cache: Arc<InMemoryCache>,
  runtime: Arc<Runtime>,
}

struct DiscordBotClientSender {
  http_client: Client,
  shard_senders: HashMap<Id<GuildMarker>, MessageSender>,
}
```

- [ ] Job Scheduler:

```rust
struct JobScheduler {
  job_scheduler: JobScheduler,
  runtime: Arc<Runtime>,
}
```

- [ ] Runtime:

```rust
pub struct PluginBuilder {
    engine: Engine,
    linker: Linker<InternalRuntime>,
}

pub struct Runtime {
    plugins: RwLock<HashMap<String, RuntimePlugin>>,
    discord_bot_client_sender: DiscordBotClientSender,
    data: Data,
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>, // No async support yet
}

struct InternalRuntime {
    ctx: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
    runtime: Arc<Runtime>,
}
```

#### Startup Sequence

1. Classic application elements (logger and more)
2. DiscordBotClientSender
3. Data
4. Runtime
5. DiscordBotClientReceiver (create and start)
6. JobScheduler (create and start)
7. Initiate plugins
