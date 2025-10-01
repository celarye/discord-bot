# Discord Bot

> [!NOTE]
> This project is still in **very early development**. While it is open source,
> it is **not open to contributions** just yet. This will change once version
> `v0.1.0` is reached.
> The current goal is to reach `v0.1.0` sometime in early to mid October.

> [!WARNING]
> The current commit history will **frequently be rewritten** until `v0.1.0` is
> released.

A WASI plugin based Discord bot, configurable through YAML.

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

# To Do List

- [ ] Codebase restructure
- [ ] Complete the job scheduler
- [ ] Implement all WASI host functions
- [ ] Implement the Discord request handler
- [ ] Add support for  other Discord events and requests
- [ ] Make plugins

### Codebase Restructure

- [ ] Plugins:

```rust
pub struct AvailablePlugin {
    pub name: String,
    pub version: String,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

#[derive(Serialize)]
pub struct PluginRegistrations {
    pub discord_events: PluginRegistrationsDiscordEvents,
    pub scheduled_jobs: HashMap<String, Vec<(String, String)>>,
    pub dependencies: HashMap<String, HashSet<String>>,
}

#[derive(Serialize)]
pub struct PluginRegistrationsDiscordEvents {
    pub interaction_create_commands: HashMap<String, (String, String)>,
    pub message_create: Vec<String>,
}

pub struct InitializedPluginRegistrations {
    pub commands: Vec<InitializedPluginRegistrationsCommand>,
    pub scheduled_jobs: Vec<InitializedPluginRegistrationsScheduledJobs>,
}

pub struct InitializedPluginRegistrationsCommand {
    pub plugin_name: String,
    pub internal_name: String,
    pub command_data: Vec<u8>,
}

pub struct InitializedPluginRegistrationsScheduledJobs {
    pub plugin_name: String,
    pub internal_name: String,
    pub cron: String,
}
```

- [ ] Discord:

```rust
pub struct DiscordBotClientReceiver {
    shards: Box<dyn ExactSizeIterator<Item = Shard>>,
    cache: Arc<InMemoryCache>,
    runtime: Arc<Runtime>,
}

pub struct DiscordBotClientSender {
    pub http_client: Client,
    pub shard_message_senders: HashMap<Id<GuildMarker>, Arc<MessageSender>>,
}
```

- [ ] Job Scheduler:

```rust
pub struct JobScheduler {
    job_scheduler: TCScheduler,
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
    plugin_registrations: RwLock<PluginRegistrations>,
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>,
}

struct InternalRuntime {
    wasi: WasiCtx,
    wasi_http: WasiHttpCtx,
    table: ResourceTable,
    runtime: Arc<Runtime>,
}

```

#### Startup Sequence

1. Classic application elements (logger and more)
2. DiscordBotClientSender
3. Runtime
4. DiscordBotClientReceiver (create and start)
5. JobScheduler (create and start)
6. Initiate plugins
