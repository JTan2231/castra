# Castra Bus Architecture

## Context
- Thread 3 in `.vizier` commits us to a reliable host↔guest broker with freshness reporting; today the broker only records one-shot handshakes (`src/core/broker.rs`, `src/core/status.rs`).
- Guests run behind QEMU user-mode NAT and already initiate outbound TCP connections to the host for the handshake, so host-mediated messaging fits the existing trust and network model.
- Operators want a way to publish messages to guests and aggregate messages from guests without introducing peer-to-peer routing or extra network plumbing.

## Goals
- Provide a host-centric publish/subscribe channel (“Castra Bus”) that guests can publish into and optionally subscribe to.
- Reuse the broker identity handshake so we retain freshness semantics (`reachable`, `last_handshake_age_ms`) while extending the broker into a lightweight message hub.
- Keep the system script-friendly: bounded startup waits, non-blocking status calls, and durable logs for observability.

## High-Level Topology
```
+------------+        +-------------------+        +------------+
| Guest VM A | <----> | Host broker & bus | <----> | Guest VM B |
|  agent     |        |   (main stream)   |        |  agent     |
+------------+        +-------------------+        +------------+
        \                                           /
         +--------------> Optional host consumers <+
```
- Each guest maintains a TCP session to the host bus for outbound publish and inbound subscription.
- The host may expose a loopback client for operators or automation to publish messages.
- All fan-out occurs on the host; guests do not talk directly to each other.

## Components & Responsibilities
- **Guest agent (per VM)**
  - Boot-time: connect to broker, perform existing `hello vm:<name>` handshake, upgrade or reopen the connection to a bus channel.
  - Runtime: publish structured events to the host stream and optionally subscribe to host broadcasts (per-VM opt-in).
  - Resilience: reconnect with exponential backoff; re-issue handshake on reconnect.

- **Host broker / bus**
  - Authentication: tie sessions to VM identities obtained during the handshake; reject unknown names.
  - Publish pipeline: accept messages from guests, tag with metadata (vm, timestamp), persist to rotating log, and replay to any host subscribers.
  - Subscription: manage per-session cursors; deliver host broadcasts downstream to subscribed guests.
  - Observability: extend status output to report per-VM subscription state and last publish age while preserving the existing reachable/age fields.

- **Host CLI / tooling**
  - `castra bus publish`: push a message into the bus (broadcast or targeted).
  - `castra bus tail`: follow bus events, similar to `castra logs`.
  - Integrate with `castra status` and `castra ports --active` to surface bus availability.

## Protocol Sketch
1. Guest connects to `host:broker_port`, reads greeting, sends `hello vm:<name> capabilities=bus`.
2. Broker persists the handshake file (existing behavior) and responds with `ok session=<token>`.
3. Guest opens/continues a session to `/bus` (same TCP connection with framed messages or a second port).
4. Messages use a simple length-prefixed JSON frame:
   ```json
   {
     "type": "publish" | "subscribe" | "ack",
     "topic": "broadcast" | "vm:<name>" | "...",
     "payload": { ... }
   }
   ```
5. Broker routes payloads:
   - `publish` from guest → append to log, fan-out to host subscribers, optional broadcast to other guests that subscribed.
   - `publish` from host → deliver to matching guest subscriptions.
6. Heartbeats (`type: "heartbeat"`) keep sessions fresh; missing heartbeats age out reachability.

## Operational Considerations
- **Performance**: keep message handling non-blocking, target <200 ms end-to-end as per Thread 6 latency guidance.
- **Persistence**: reuse existing log directory structure, rotate bus logs per VM and per host stream.
- **Security**: the bus runs on localhost; rely on VM identity for channel isolation. Future work could add shared secrets or TLS if needed.
- **Failure modes**: guest disconnects invalidate its subscription and mark reachability as stale. Broker restarts purge sessions; guests must reconnect automatically.

## Incremental Delivery
1. Extend broker to maintain long-lived sessions and expose a framed bus API while keeping current handshake behavior (Thread 3 acceptance).
2. Ship a guest agent update that upgrades to the bus protocol; validate reachability and freshness stay accurate.
3. Add host CLI commands for publish/tail and document the bus, including scripting guarantees.
4. Optional: introduce topics, targeted messages, and replay controls once baseline broadcast works.

## Risks & Mitigations
- **Session leaks**: enforce idle timeouts and drop orphaned connections.
- **Back-pressure**: rate-limit per guest and buffer host broadcasts carefully; expose diagnostics when limits hit.
- **Compatibility**: version the protocol (`capabilities=bus-v1`) so older guests stay on handshake-only mode until they upgrade.
