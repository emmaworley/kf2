# KF2 Providers Framework

## Overview

All logic associated with upstream karaoke data providers are contained in the `provider` module. This is further divided into submodules containing logic specific to each provider. Since different providers have different capabilities, provider functionality is expressed as a composition of traits (e.g. `Searchable`, `LyricsProvider`). This allows a somewhat-uniform API surface even across wildly different feature landscapes.

## Lifecycle

Each provider is statically registered with `ProviderRegistry` on startup. The list of available providers is fixed at compile-time. (TODO: is hardcoded registration necessary, can we use reflection or something similar?) The static provider instances hold data and logic that persist for the lifetime of the application (e.g. YouTube searches for yt-dlp on startup). Providers can be dynamically enabled/disabled on startup (e.g. YouTube might be disabled if it can't find a usable yt-dlp).

Per session, the global provider is instanced into separate `ProviderSession`s that implement functionality traits and store per-session data, like credentials. These `ProviderSession`s perform the bulk of the logic: making API calls to the upstreams, transforming data, and interfacing with other parts of the server like the asset cache subsystem.

## gRPC API Surface

Karaoke provider functionality is primarily exposed through two separate gRPC services.

### ProviderService

Provides global (server-wide) information about available providers. The primary remit of this service is server introspection (for UI/CLI usage).

### SessionService

Heavy service that drives the main karaoke session logic. Composes server logic into a single interface that the projector and remocon SPAs interact with. 