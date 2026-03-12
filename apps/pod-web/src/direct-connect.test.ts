import { describe, expect, test } from "bun:test";

import {
  initialHudStateFromLocation,
  PodWebDirectConnectClient,
  runtimeConfigFromLocation
} from "./direct-connect";

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readonly sent: string[] = [];
  readyState = MockWebSocket.CONNECTING;
  private readonly listeners = new Map<string, Array<(event: unknown) => void>>();

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  send(payload: string): void {
    this.sent.push(payload);
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.emit("close", {});
  }

  open(): void {
    this.readyState = MockWebSocket.OPEN;
    this.emit("open", {});
  }

  emitMessage(payload: string): void {
    this.emit("message", { data: payload });
  }

  private emit(type: string, event: unknown): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

describe("direct-connect runtime config", () => {
  test("builds a websocket config from server query params", () => {
    const config = runtimeConfigFromLocation({
      search:
        "?server=127.0.0.1:7778&player=Scout&debug=1&reconnectMs=2500&pingMs=900&heartbeatMs=4500&pendingBatches=4"
    } as Location);

    expect(config).not.toBeNull();
    expect(config?.url).toBe("ws://127.0.0.1:7778");
    expect(config?.playerName).toBe("Scout");
    expect(config?.debugTelemetry).toBe(true);
    expect(config?.reconnectDelayMs).toBe(2500);
    expect(config?.pingIntervalMs).toBe(900);
    expect(config?.heartbeatTimeoutMs).toBe(4500);
    expect(config?.maxPendingActionBatches).toBe(4);
  });

  test("returns null when no direct-connect params are present", () => {
    const config = runtimeConfigFromLocation({
      search: "?demo=1"
    } as Location);

    expect(config).toBeNull();
  });

  test("describes local sandbox boot state when no runtime config is present", () => {
    const state = initialHudStateFromLocation({
      search: ""
    } as Location);

    expect(state.connectionBadge).toBe("local sandbox booting");
    expect(state.worldLabel).toBe("booting Verdant Hollow");
    expect(state.frameSourceLabel).toBe("bootstrapping local sandbox");
  });

  test("describes bootstrap showcase boot state from local world query params", () => {
    const state = initialHudStateFromLocation({
      search: "?world=bootstrap-showcase"
    } as Location);

    expect(state.feedback).toBe("Staging bootstrap showcase");
    expect(state.connectionBadge).toBe("showcase route booting");
    expect(state.worldLabel).toBe("booting Resonant Shore");
    expect(state.frameSourceLabel).toBe("bootstrapping showcase shard");
  });

  test("describes direct-connect boot state from query params", () => {
    const state = initialHudStateFromLocation({
      search: "?server=127.0.0.1:7778"
    } as Location);

    expect(state.feedback).toBe("Connecting to ws://127.0.0.1:7778");
    expect(state.connectionBadge).toBe("connecting to shard");
    expect(state.worldLabel).toBe("waiting for shard snapshot");
  });

  test("submits websocket action batches after an authoritative welcome", () => {
    const originalWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    MockWebSocket.instances = [];

    const frames: number[] = [];
    const eventSummaries: string[] = [];
    const debugDocuments: string[] = [];
    const actionStates: Array<{
      pendingCount: number;
      lastSubmittedTick: number | null;
      lastAcknowledgedTick: number | null;
      lastRejectedTick: number | null;
      lastRejectedReason: string | null;
      lastActionSummary: string | null;
    }> = [];
    const statuses: Array<{
      phase: string;
      roundTripMs: number | null;
      jitterMs: number | null;
      lastPongServerTick: number | null;
    }> = [];
    const originalDateNow = Date.now;

    try {
      let now = 10_000;
      Date.now = () => now;
      const client = new PodWebDirectConnectClient(
        {
          url: "ws://127.0.0.1:7778",
          playerName: "BrowserPilot",
          debugTelemetry: true,
          reconnectDelayMs: 1000,
          pingIntervalMs: 5000,
          heartbeatTimeoutMs: 6500,
          maxPendingActionBatches: 6
        },
        {
          onFrame(snapshot) {
            frames.push(snapshot.tick);
          },
          onEventBatch(batch) {
            eventSummaries.push(...batch.events.map((event) => event.summary));
          },
          onDebugDocument(document) {
            debugDocuments.push(document);
          },
          onActionState(state) {
            actionStates.push(state);
          },
          onStatus(status) {
            statuses.push({
              phase: status.phase,
              roundTripMs: status.roundTripMs,
              jitterMs: status.jitterMs,
              lastPongServerTick: status.lastPongServerTick
            });
          }
        }
      );

      client.connect();
      const socket = MockWebSocket.instances[0];
      expect(socket).toBeDefined();
      socket?.open();

      expect(socket?.sent.slice(0, 3)).toEqual([
        JSON.stringify({
          Connect: {
            player_name: "BrowserPilot",
            reconnect_token: null
          }
        }),
        JSON.stringify({
          SetDebugTelemetry: {
            enabled: true
          }
        }),
        JSON.stringify({
          SetDebugFocus: {
            entity_id: null
          }
        })
      ]);
      expect(JSON.parse(socket?.sent[3] ?? "null")).toEqual({
        Ping: {
          timestamp: 10_000
        }
      });

      socket?.emitMessage(
        JSON.stringify({
          Welcome: {
            client_id: "client-1",
            reconnect_token: "resume-1",
            tick: 18,
            controlled_entity: 12,
            authoritative_digest: 999,
            snapshot: {
              tick: 18,
              entities: [
                {
                  id: 12,
                  position: [10, 10],
                  velocity: [0, 0],
                  rotation: 0,
                  label: "Hero"
                }
              ]
            }
          }
        })
      );

      expect(frames).toEqual([18]);

      const sent = client.submitActions([{ kind: "move", direction: [1, 0] }]);
      expect(sent).toBe(true);
      expect(socket?.sent.at(-1)).toEqual(
        JSON.stringify({
          ActionBatch: {
            tick: 19,
            actions: [{ Move: { direction: [1, 0] } }]
          }
        })
      );
      expect(actionStates.at(-1)).toEqual({
        pendingCount: 1,
        lastSubmittedTick: 19,
        lastAcknowledgedTick: null,
        lastRejectedTick: null,
        lastRejectedReason: null,
        lastActionSummary: "move"
      });

      socket?.emitMessage(
        JSON.stringify({
          StateDelta: {
            tick: 19,
            acknowledged_action_tick: 19,
            authoritative_digest: 1001,
            is_full_snapshot: false,
            delta: {
              tick: 19,
              updated: [
                {
                  id: 12,
                  position: [11, 10],
                  velocity: [1, 0],
                  rotation: 0,
                  label: "Hero"
                }
              ],
              destroyed: []
            }
          }
        })
      );
      expect(actionStates.at(-1)).toEqual({
        pendingCount: 0,
        lastSubmittedTick: 19,
        lastAcknowledgedTick: 19,
        lastRejectedTick: null,
        lastRejectedReason: null,
        lastActionSummary: "move"
      });

      client.submitActions([{ kind: "speak", message: "hello", volume: "Normal" }]);
      socket?.emitMessage(
        JSON.stringify({
          Rejected: {
            reason: "speak cooldown"
          }
        })
      );
      expect(actionStates.at(-1)).toEqual({
        pendingCount: 0,
        lastSubmittedTick: 20,
        lastAcknowledgedTick: 19,
        lastRejectedTick: 20,
        lastRejectedReason: "speak cooldown",
        lastActionSummary: "speak"
      });

      socket?.emitMessage(
        JSON.stringify({
          EventBatch: {
            tick: 19,
            events: [
              {
                tick: 19,
                origin: [12, 10],
                event: {
                  LootClaimed: {
                    entity: 12,
                    source: 14,
                    coins: 32,
                    item_count: 2
                  }
                }
              }
            ]
          }
        })
      );
      expect(eventSummaries).toEqual(["E(12) looted 32 coins"]);

      socket?.emitMessage(
        JSON.stringify({
          DebugDocument: {
            document: "debug-doc"
          }
        })
      );
      expect(debugDocuments).toEqual(["debug-doc"]);

      now = 10_064;
      socket?.emitMessage(
        JSON.stringify({
          Pong: {
            client_ts: 10_000,
            server_ts: 19
          }
        })
      );
      expect(statuses.at(-1)).toEqual({
        phase: "connected",
        roundTripMs: 64,
        jitterMs: 0,
        lastPongServerTick: 19
      });

      client.setDebugFocusEntity(12);
      expect(socket?.sent.at(-1)).toEqual(
        JSON.stringify({
          SetDebugFocus: {
            entity_id: 12
          }
        })
      );
    } finally {
      Date.now = originalDateNow;
      globalThis.WebSocket = originalWebSocket;
    }
  });

  test("requests recovery when the client-side action backlog saturates", () => {
    const originalWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    MockWebSocket.instances = [];

    const actionStates: Array<{
      pendingCount: number;
      lastSubmittedTick: number | null;
      lastAcknowledgedTick: number | null;
      lastRejectedTick: number | null;
      lastRejectedReason: string | null;
      lastActionSummary: string | null;
    }> = [];

    try {
      const client = new PodWebDirectConnectClient(
        {
          url: "ws://127.0.0.1:7778",
          playerName: "BrowserPilot",
          debugTelemetry: false,
          reconnectDelayMs: 1000,
          pingIntervalMs: 5000,
          heartbeatTimeoutMs: 6500,
          maxPendingActionBatches: 1
        },
        {
          onFrame() {},
          onEventBatch() {},
          onDebugDocument() {},
          onActionState(state) {
            actionStates.push(state);
          },
          onStatus() {}
        }
      );

      client.connect();
      const socket = MockWebSocket.instances[0];
      socket?.open();
      socket?.emitMessage(
        JSON.stringify({
          Welcome: {
            client_id: "client-1",
            reconnect_token: "resume-1",
            tick: 18,
            controlled_entity: 12,
            authoritative_digest: 999,
            snapshot: {
              tick: 18,
              entities: [
                {
                  id: 12,
                  position: [10, 10],
                  velocity: [0, 0],
                  rotation: 0,
                  label: "Hero"
                }
              ]
            }
          }
        })
      );

      expect(client.submitActions([{ kind: "move", direction: [1, 0] }])).toBe(true);
      expect(
        client.submitActions([{ kind: "move", direction: [0, 1] }])
      ).toBe(false);
      expect(JSON.parse(socket?.sent.at(-1) ?? "null")).toEqual({
        RequestFullSnapshot: {
          last_known_tick: 18,
          last_known_digest: 999
        }
      });
      expect(actionStates.at(-1)?.lastRejectedReason).toBe(
        "client backlog saturated (1)"
      );
    } finally {
      globalThis.WebSocket = originalWebSocket;
    }
  });

  test("forces reconnect when authority heartbeat times out", () => {
    const originalWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    MockWebSocket.instances = [];
    const originalDateNow = Date.now;
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;

    const statuses: string[] = [];
    const scheduledReconnects: Array<() => void> = [];

    try {
      let now = 20_000;
      Date.now = () => now;
      globalThis.setTimeout = ((handler: TimerHandler) => {
        if (typeof handler === "function") {
          scheduledReconnects.push(handler as () => void);
        }
        return scheduledReconnects.length as unknown as ReturnType<typeof setTimeout>;
      }) as typeof setTimeout;
      globalThis.clearTimeout = (() => {}) as typeof clearTimeout;
      const client = new PodWebDirectConnectClient(
        {
          url: "ws://127.0.0.1:7778",
          playerName: "BrowserPilot",
          debugTelemetry: false,
          reconnectDelayMs: 1000,
          pingIntervalMs: 5000,
          heartbeatTimeoutMs: 2500,
          maxPendingActionBatches: 6
        },
        {
          onFrame() {},
          onEventBatch() {},
          onDebugDocument() {},
          onActionState() {},
          onStatus(status) {
            statuses.push(`${status.phase}:${status.detail}`);
          }
        }
      );

      client.connect();
      const socket = MockWebSocket.instances[0];
      socket?.open();
      socket?.emitMessage(
        JSON.stringify({
          Welcome: {
            client_id: "client-1",
            reconnect_token: "resume-1",
            tick: 18,
            controlled_entity: 12,
            authoritative_digest: 999,
            snapshot: {
              tick: 18,
              entities: [
                {
                  id: 12,
                  position: [10, 10],
                  velocity: [0, 0],
                  rotation: 0,
                  label: "Hero"
                }
              ]
            }
          }
        })
      );

      now = 22_800;
      client.updateNetworkHealth(now);

      expect(client.currentStatus().phase).toBe("reconnecting");
      expect(statuses.some((status) => status.includes("heartbeat timed out"))).toBe(
        true
      );
      expect(socket?.readyState).toBe(MockWebSocket.CLOSED);
      expect(scheduledReconnects).toHaveLength(1);

      scheduledReconnects[0]?.();
      const resumedSocket = MockWebSocket.instances[1];
      resumedSocket?.open();

      expect(JSON.parse(resumedSocket?.sent[0] ?? "null")).toEqual({
        Connect: {
          player_name: "BrowserPilot",
          reconnect_token: "resume-1"
        }
      });
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
      Date.now = originalDateNow;
      globalThis.WebSocket = originalWebSocket;
    }
  });

  test("forces reconnect instead of requesting recovery when backlog saturates under stale authority", () => {
    const originalWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    MockWebSocket.instances = [];
    const originalDateNow = Date.now;
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;

    const statuses: string[] = [];
    const scheduledReconnects: Array<() => void> = [];

    try {
      let now = 30_000;
      Date.now = () => now;
      globalThis.setTimeout = ((handler: TimerHandler) => {
        if (typeof handler === "function") {
          scheduledReconnects.push(handler as () => void);
        }
        return scheduledReconnects.length as unknown as ReturnType<typeof setTimeout>;
      }) as typeof setTimeout;
      globalThis.clearTimeout = (() => {}) as typeof clearTimeout;

      const client = new PodWebDirectConnectClient(
        {
          url: "ws://127.0.0.1:7778",
          playerName: "BrowserPilot",
          debugTelemetry: false,
          reconnectDelayMs: 1000,
          pingIntervalMs: 5000,
          heartbeatTimeoutMs: 4000,
          maxPendingActionBatches: 1
        },
        {
          onFrame() {},
          onEventBatch() {},
          onDebugDocument() {},
          onActionState() {},
          onStatus(status) {
            statuses.push(`${status.phase}:${status.detail}`);
          }
        }
      );

      client.connect();
      const socket = MockWebSocket.instances[0];
      socket?.open();
      socket?.emitMessage(
        JSON.stringify({
          Welcome: {
            client_id: "client-1",
            reconnect_token: "resume-1",
            tick: 18,
            controlled_entity: 12,
            authoritative_digest: 999,
            snapshot: {
              tick: 18,
              entities: [
                {
                  id: 12,
                  position: [10, 10],
                  velocity: [0, 0],
                  rotation: 0,
                  label: "Hero"
                }
              ]
            }
          }
        })
      );

      expect(client.submitActions([{ kind: "move", direction: [1, 0] }])).toBe(true);
      now = 32_100;
      expect(client.submitActions([{ kind: "move", direction: [0, 1] }])).toBe(false);

      expect(client.currentStatus().phase).toBe("reconnecting");
      expect(
        statuses.some((status) =>
          status.includes("Action backlog saturated (1) under stale authority")
        )
      ).toBe(true);
      expect(socket?.readyState).toBe(MockWebSocket.CLOSED);
      expect(
        socket?.sent.some((payload) => payload.includes("RequestFullSnapshot"))
      ).toBe(false);
      expect(scheduledReconnects).toHaveLength(1);
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
      Date.now = originalDateNow;
      globalThis.WebSocket = originalWebSocket;
    }
  });
});
