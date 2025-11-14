# Discord Bot

A WASI plugin based Discord bot, configurable through YAML.

## Project Goal

The goal of this project is to create a **Docker Compose-like experience** for
Discord bots.

Users are able to self-host their own personal bot, assembled from plugins
available from the official registry.

This official registry contains plugins covering the most common features
required from Discord bots.

Programmers are also able to add their own custom plugins. This allows them
to focus on what really matters (the functionality of that plugin) while
relying on features provided by other plugins.

## To Do List

- [X] Codebase restructure
- [X] Complete the job scheduler
- [X] Implement all WASI host functions
- [X] Implement the Discord request handler
- [ ] Microservice based daemon rewrite
- [ ] Implement all TODOs
- [ ] Add support for all Discord events and requests
- [ ] Make plugins
