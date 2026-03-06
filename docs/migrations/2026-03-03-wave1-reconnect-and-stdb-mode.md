# Migration: Wave 1 reconnect protocol + StDB connection mode

Date: 2026-03-03  
Scope: `pod-net`, `pod-stdb`

## Breaking changes

1. `pod-net` protocol `ClientMessage::Connect` now requires a `reconnect_token` field:
- Previous:
```rust
ClientMessage::Connect { player_name }
```
- Current:
```rust
ClientMessage::Connect {
    player_name,
    reconnect_token: Option<ReconnectToken>,
}
```

2. `pod-net` protocol `ServerMessage::Welcome` now includes `reconnect_token`:
- Previous:
```rust
ServerMessage::Welcome { client_id, tick, snapshot }
```
- Current:
```rust
ServerMessage::Welcome {
    client_id,
    reconnect_token,
    tick,
    snapshot,
}
```

3. `pod-stdb` client config adds explicit runtime mode:
- New field:
```rust
StdbClientConfig {
    connection_mode: StdbConnectionMode,
    // ...
}
```

## Runtime behavior changes

1. Native/web action batches are rejected if:
- Tick is outside ingress window.
- Tick is stale (decreasing per-client submission order).

2. Web/native reconnect now reuses server-issued `ReconnectToken` when available.

3. `pod-server` runtime default is now `network` when `POD_RUNTIME_MODE` is unset.

## `pod-stdb` mode defaults

1. Debug builds default to:
- `StdbConnectionMode::Emulated`

2. Release builds default to:
- `StdbConnectionMode::Generated`

3. If `Generated` mode is selected without generated runtime wiring in the build:
- `connect()` returns `StdbError::ConnectionFailed(...)`
- Client state transitions to `ConnectionState::Error(...)`

## Required caller updates

1. Handle/connect with reconnect token:
- Persist `ReconnectToken` received in `Welcome`.
- Re-send it on reconnect through `ClientMessage::Connect`.

2. When constructing `StdbClientConfig` via struct literal:
- Add `connection_mode`, or use `..StdbClientConfig::default()`.

3. For `SpacetimeDBClientConfig` (`pod-net`):
- `connection_mode` is now forwarded to `StdbClientConfig`.

