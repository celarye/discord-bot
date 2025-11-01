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

## To Do List

- [X] Codebase restructure
- [X] Complete the job scheduler
- [X] Implement all WASI host functions
- [X] Implement the Discord request handler
- [ ] Implement all TODOs
- [ ] Add support for other Discord events and requests
- [ ] Make plugins
