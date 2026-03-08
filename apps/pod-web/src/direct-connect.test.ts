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
      search: "?server=127.0.0.1:7778&player=Scout&debug=1&reconnectMs=2500"
    } as Location);

    expect(config).not.toBeNull();
    expect(config?.url).toBe("ws://127.0.0.1:7778");
    expect(config?.playerName).toBe("Scout");
    expect(config?.debugTelemetry).toBe(true);
    expect(config?.reconnectDelayMs).toBe(2500);
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

    try {
      const client = new PodWebDirectConnectClient(
        {
          url: "ws://127.0.0.1:7778",
          playerName: "BrowserPilot",
          debugTelemetry: true,
          reconnectDelayMs: 1000
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
          onStatus() {}
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

      client.setDebugFocusEntity(12);
      expect(socket?.sent.at(-1)).toEqual(
        JSON.stringify({
          SetDebugFocus: {
            entity_id: 12
          }
        })
      );
    } finally {
      globalThis.WebSocket = originalWebSocket;
    }
  });
});
