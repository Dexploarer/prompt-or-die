import type {
  BrowserAction,
  BrowserCompanionCommand,
  NetworkCombatStyle,
  NetworkEncounterKind,
  NetworkEntityInteractionHints,
  NetworkEntityKind,
  NetworkEntityMetadataSnapshot,
  NetworkEntitySnapshot,
  NetworkEventBatch,
  NetworkGameEvent,
  NetworkSkillKind,
  NetworkWorldSnapshot,
  Vec2Tuple
} from "./contracts";
import type {
  DirectConnectActionState,
  DirectConnectStatus
} from "./direct-connect";

const LOCAL_WORLD_URL = "local://verdant-hollow";
const LOCAL_WORLD_NAME = "Verdant Hollow";
const LOCAL_TICK_MS = 1000 / 60;
const PLAYER_ID = 1;
const MELEE_RANGE = 3.1;
const COMPANION_RANGE = 2.8;
const PLAYER_ATTACK_COOLDOWN = 30;
const CREATURE_ATTACK_COOLDOWN = 40;
const COMPANION_ATTACK_COOLDOWN = 26;
const WORLD_LIMIT = 11.5;

type LocalEntityRole =
  | "player"
  | "npc"
  | "wild"
  | "resource"
  | "loot"
  | "companion"
  | "scenery";

type CompanionMode = "Follow" | "Guard" | "Attack";

interface LocalEntity {
  id: number;
  role: LocalEntityRole;
  label: string;
  position: Vec2Tuple;
  velocity: Vec2Tuple;
  rotation: number;
  movementSpeed: number;
  health: number | null;
  maxHealth: number | null;
  metadata: NetworkEntityMetadataSnapshot;
  spawn: Vec2Tuple;
  desiredMove: Vec2Tuple | null;
  combatTargetId: number | null;
  attackCooldownTicks: number;
  interactionRadius: number;
  resourceRemaining: number;
  lootCoins: number;
  lootItemId: string | null;
  companionSlot: number | null;
  companionMode: CompanionMode | null;
  anchor: Vec2Tuple | null;
}

interface CompanionRosterEntry {
  speciesId: string;
  speciesName: string;
}

interface PendingActionBatch {
  tick: number;
  actions: BrowserAction[];
  summary: string;
}

interface LocalWorldState {
  tick: number;
  entities: LocalEntity[];
  nextEntityId: number;
  companionRoster: CompanionRosterEntry[];
  activeCompanionId: number | null;
  autoRetaliate: boolean;
}

export class PodWebLocalWorld {
  private state: LocalWorldState;
  private accumulatorMs = 0;
  private connected = false;
  private pendingActions: PendingActionBatch[] = [];
  private pendingEvents: NetworkGameEvent[] = [];
  private status: DirectConnectStatus;
  private actionState: DirectConnectActionState;

  constructor(private readonly playerName = "WebPlayer") {
    this.state = createInitialState(playerName);
    this.status = createStatus(this.state);
    this.actionState = createActionState();
  }

  connect(): void {
    this.connected = true;
    this.status = {
      ...this.status,
      phase: "connected",
      detail: "Local sandbox shard ready",
      tick: this.state.tick,
      entityCount: this.state.entities.length,
      controlledEntity: PLAYER_ID
    };
  }

  reset(): void {
    this.state = createInitialState(this.playerName);
    this.accumulatorMs = 0;
    this.pendingActions = [];
    this.pendingEvents = [];
    this.actionState = createActionState();
    this.status = createStatus(this.state);
    if (this.connected) {
      this.connect();
    }
  }

  step(deltaMs: number): void {
    if (!this.connected) {
      return;
    }

    this.accumulatorMs += Math.max(0, deltaMs);
    while (this.accumulatorMs >= LOCAL_TICK_MS) {
      this.accumulatorMs -= LOCAL_TICK_MS;
      this.runTick();
    }
  }

  submitActions(actions: BrowserAction[]): boolean {
    if (!this.connected || actions.length === 0) {
      return false;
    }

    const tick = this.state.tick + this.pendingActions.length + 1;
    const summary = summarizeActions(actions);
    this.pendingActions.push({ tick, actions, summary });
    this.actionState = {
      ...this.actionState,
      pendingCount: this.pendingActions.length,
      lastSubmittedTick: tick,
      lastRejectedReason: null,
      lastActionSummary: summary
    };
    return true;
  }

  snapshotState(): NetworkWorldSnapshot {
    return {
      tick: this.state.tick,
      entities: this.state.entities
        .map((entity) => toSnapshot(entity))
        .sort((left, right) => left.id - right.id)
    };
  }

  currentStatus(): DirectConnectStatus {
    return { ...this.status };
  }

  currentActionState(): DirectConnectActionState {
    return { ...this.actionState };
  }

  controlledEntityId(): number {
    return PLAYER_ID;
  }

  drainEventBatch(): NetworkEventBatch | null {
    if (this.pendingEvents.length === 0) {
      return null;
    }

    const batch = {
      tick: this.state.tick,
      events: this.pendingEvents.slice()
    };
    this.pendingEvents = [];
    return batch;
  }

  companionRoster(): CompanionRosterEntry[] {
    return this.state.companionRoster.slice();
  }

  private runTick(): void {
    this.state.tick += 1;
    const events: NetworkGameEvent[] = [];

    this.processPendingActions(events);
    this.updateAutonomousBehaviors();
    this.integrateMovement();
    this.resolveCombat(events);

    if (events.length > 0) {
      this.pendingEvents.push(...events);
    }

    this.status = {
      ...this.status,
      tick: this.state.tick,
      entityCount: this.state.entities.length,
      controlledEntity: PLAYER_ID,
      authoritativeDigest: this.state.tick
    };
  }

  private processPendingActions(events: NetworkGameEvent[]): void {
    const ready = this.pendingActions.filter((batch) => batch.tick <= this.state.tick);
    this.pendingActions = this.pendingActions.filter((batch) => batch.tick > this.state.tick);

    for (const batch of ready) {
      let rejectedReason: string | null = null;
      for (const action of batch.actions) {
        const rejection = this.applyAction(action, events);
        if (!rejectedReason && rejection) {
          rejectedReason = rejection;
        }
      }

      if (rejectedReason) {
        this.actionState = {
          ...this.actionState,
          pendingCount: this.pendingActions.length,
          lastRejectedTick: batch.tick,
          lastRejectedReason: rejectedReason,
          lastActionSummary: batch.summary
        };
      } else {
        this.actionState = {
          ...this.actionState,
          pendingCount: this.pendingActions.length,
          lastAcknowledgedTick: batch.tick,
          lastRejectedReason: null,
          lastActionSummary: batch.summary
        };
      }
    }
  }

  private applyAction(action: BrowserAction, events: NetworkGameEvent[]): string | null {
    const player = requireEntity(this.state, PLAYER_ID);

    switch (action.kind) {
      case "move":
        player.desiredMove = normalize(action.direction);
        return null;
      case "stop":
        player.desiredMove = null;
        player.combatTargetId = null;
        return null;
      case "rotate":
        player.rotation = action.angle;
        return null;
      case "lookAt":
        player.rotation = angleBetween(player.position, action.target);
        return null;
      case "attack":
        return "Select a target before attacking";
      case "attackTarget": {
        const target = this.findEntity(action.target);
        if (!target || !target.metadata.interaction.canAttack) {
          return "Target cannot be attacked";
        }
        if (!withinRange(player.position, target.position, MELEE_RANGE)) {
          return "Move closer to attack";
        }
        player.combatTargetId = target.id;
        if (target.role === "wild") {
          target.combatTargetId = player.id;
        }
        this.performAttack(player, target, events, MELEE_RANGE, PLAYER_ATTACK_COOLDOWN);
        return null;
      }
      case "interact":
        return "Select a target before interacting";
      case "interactWith": {
        const target = this.findEntity(action.target);
        if (!target || !target.metadata.interaction.canInspect) {
          return "Target cannot be inspected";
        }
        if (!withinRange(player.position, target.position, target.interactionRadius + 0.8)) {
          return "Move closer to inspect";
        }
        events.push(eventRecord(this.state.tick, player.position, "Interact", describeInteraction(target), [
          player.id,
          target.id
        ]));
        return null;
      }
      case "gatherResource": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "resource" || !target.metadata.interaction.canGather) {
          return "Target cannot be gathered";
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return "Move closer to gather";
        }
        target.resourceRemaining = Math.max(0, target.resourceRemaining - 1);
        if (target.resourceRemaining === 0) {
          target.metadata.interaction.canGather = false;
          target.label = `Depleted ${target.label}`;
        }
        events.push(
          eventRecord(
            this.state.tick,
            target.position,
            "ResourceGathered",
            `E(${player.id}) gathered 1 ${target.lootItemId ?? action.skill.toLowerCase()}`,
            [player.id, target.id]
          )
        );
        return null;
      }
      case "loot": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "loot" || !target.metadata.interaction.canLoot) {
          return "Target has no loot";
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return "Move closer to loot";
        }
        events.push(
          eventRecord(
            this.state.tick,
            target.position,
            "LootClaimed",
            `E(${player.id}) looted ${target.lootCoins} coins`,
            [player.id, target.id]
          )
        );
        this.destroyEntity(target.id);
        return null;
      }
      case "captureCreature": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "wild" || !target.metadata.interaction.canCapture) {
          return "Target cannot be captured";
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return "Move closer to capture";
        }
        if ((target.health ?? 0) > (target.maxHealth ?? 0) * 0.5) {
          return "Weaken the creature first";
        }
        if (this.state.companionRoster.length >= 3) {
          return "Companion roster is full";
        }
        this.state.companionRoster.push({
          speciesId: target.metadata.speciesId ?? slug(target.label),
          speciesName: target.metadata.speciesName ?? target.label
        });
        events.push(
          eventRecord(
            this.state.tick,
            target.position,
            "CreatureCaptured",
            `Captured ${target.metadata.speciesName ?? target.label}`,
            [player.id, target.id]
          )
        );
        this.destroyEntity(target.id);
        player.combatTargetId = null;
        return null;
      }
      case "summonCompanion": {
        const companion = this.state.companionRoster[action.slot];
        if (!companion) {
          return "No companion in that slot";
        }
        if (this.state.activeCompanionId != null && this.findEntity(this.state.activeCompanionId)) {
          return "A companion is already active";
        }
        const entity = this.spawnCompanion(companion, action.slot, player.position);
        this.state.activeCompanionId = entity.id;
        events.push(
          eventRecord(
            this.state.tick,
            entity.position,
            "CompanionSummoned",
            `Summoned ${companion.speciesName}`,
            [player.id, entity.id]
          )
        );
        return null;
      }
      case "commandCompanion": {
        const companion = this.state.activeCompanionId != null
          ? this.findEntity(this.state.activeCompanionId)
          : null;
        if (!companion || companion.role !== "companion") {
          return "No active companion";
        }
        if (action.command === "Recall") {
          events.push(
            eventRecord(
              this.state.tick,
              companion.position,
              "CompanionCommandIssued",
              "Companion Recall",
              [player.id, companion.id]
            )
          );
          this.destroyEntity(companion.id);
          this.state.activeCompanionId = null;
          return null;
        }
        companion.companionMode = action.command;
        companion.combatTargetId =
          action.command === "Attack" ? action.target ?? null : null;
        events.push(
          eventRecord(
            this.state.tick,
            companion.position,
            "CompanionCommandIssued",
            `Companion ${action.command}${action.target != null ? ` E(${action.target})` : ""}`,
            [player.id, companion.id, ...(action.target != null ? [action.target] : [])]
          )
        );
        return null;
      }
      case "speak":
        events.push(
          eventRecord(
            this.state.tick,
            player.position,
            "AgentSpoke",
            `${this.playerName}: ${action.message}`,
            [player.id]
          )
        );
        return null;
      case "setAutoRetaliate":
        this.state.autoRetaliate = action.enabled;
        events.push(
          eventRecord(
            this.state.tick,
            player.position,
            "AutoRetaliateSet",
            `E(${player.id}) auto-retaliate ${action.enabled ? "enabled" : "disabled"}`,
            [player.id]
          )
        );
        return null;
      case "idle":
        return null;
    }
  }

  private updateAutonomousBehaviors(): void {
    const player = requireEntity(this.state, PLAYER_ID);

    for (const entity of this.state.entities) {
      if (entity.id === PLAYER_ID) {
        continue;
      }

      if (entity.attackCooldownTicks > 0) {
        entity.attackCooldownTicks -= 1;
      }

      switch (entity.role) {
        case "wild":
          this.updateWildCreature(entity, player);
          break;
        case "companion":
          this.updateCompanion(entity, player);
          break;
        default:
          entity.desiredMove = null;
          entity.velocity = [0, 0];
          break;
      }
    }

    if (player.attackCooldownTicks > 0) {
      player.attackCooldownTicks -= 1;
    }
  }

  private updateWildCreature(entity: LocalEntity, player: LocalEntity): void {
    const hasPlayerAggro =
      player.combatTargetId === entity.id ||
      entity.combatTargetId === player.id ||
      withinRange(player.position, entity.position, 3.4);

    if (hasPlayerAggro && (entity.health ?? 0) > 0) {
      entity.combatTargetId = player.id;
      entity.desiredMove = moveToward(entity.position, player.position);
      return;
    }

    entity.combatTargetId = null;
    const angle = this.state.tick * 0.028 + entity.id * 0.6;
    const anchor = entity.anchor ?? entity.spawn;
    const target: Vec2Tuple = [anchor[0] + Math.cos(angle) * 1.4, anchor[1] + Math.sin(angle) * 1.1];
    entity.desiredMove = moveToward(entity.position, target);
  }

  private updateCompanion(entity: LocalEntity, player: LocalEntity): void {
    if (entity.companionMode === "Attack" && entity.combatTargetId != null) {
      const target = this.findEntity(entity.combatTargetId);
      if (target) {
        entity.desiredMove = moveToward(entity.position, target.position);
        return;
      }
    }

    const followDistance = entity.companionMode === "Guard" ? 2.6 : 1.8;
    if (!withinRange(entity.position, player.position, followDistance)) {
      entity.desiredMove = moveToward(entity.position, player.position);
    } else {
      entity.desiredMove = null;
      entity.velocity = [0, 0];
    }
  }

  private integrateMovement(): void {
    for (const entity of this.state.entities) {
      const desired = entity.desiredMove;
      if (desired) {
        entity.velocity = [desired[0] * entity.movementSpeed, desired[1] * entity.movementSpeed];
      } else {
        entity.velocity = [0, 0];
      }

      entity.position = [
        clamp(entity.position[0] + entity.velocity[0] / 60, -WORLD_LIMIT, WORLD_LIMIT),
        clamp(entity.position[1] + entity.velocity[1] / 60, -WORLD_LIMIT, WORLD_LIMIT)
      ];

      if (entity.velocity[0] !== 0 || entity.velocity[1] !== 0) {
        entity.rotation = Math.atan2(entity.velocity[0], entity.velocity[1]);
      }
    }
  }

  private resolveCombat(events: NetworkGameEvent[]): void {
    const player = requireEntity(this.state, PLAYER_ID);

    for (const entity of this.state.entities.slice()) {
      if (entity.combatTargetId == null) {
        continue;
      }

      const target = this.findEntity(entity.combatTargetId);
      if (!target || target.health == null || target.maxHealth == null) {
        entity.combatTargetId = null;
        continue;
      }

      const range = entity.role === "companion" ? COMPANION_RANGE : MELEE_RANGE;
      const cooldown =
        entity.role === "wild"
          ? CREATURE_ATTACK_COOLDOWN
          : entity.role === "companion"
            ? COMPANION_ATTACK_COOLDOWN
            : PLAYER_ATTACK_COOLDOWN;

      this.performAttack(entity, target, events, range, cooldown);

      if (target.id === player.id && this.state.autoRetaliate && player.combatTargetId == null) {
        player.combatTargetId = entity.id;
      }
    }
  }

  private performAttack(
    attacker: LocalEntity,
    target: LocalEntity,
    events: NetworkGameEvent[],
    range: number,
    cooldownTicks: number
  ): void {
    if (
      attacker.attackCooldownTicks > 0 ||
      attacker.health == null ||
      target.health == null ||
      !withinRange(attacker.position, target.position, range)
    ) {
      return;
    }

    attacker.attackCooldownTicks = cooldownTicks;
    const damage = baseDamage(attacker, this.state.tick, target.id);
    target.health = Math.max(0, target.health - damage);
    events.push(
      eventRecord(
        this.state.tick,
        target.position,
        "Damage",
        `E(${attacker.id}) hit E(${target.id}) for ${damage.toFixed(1)}`,
        [attacker.id, target.id]
      )
    );

    if (target.health > 0) {
      return;
    }

    events.push(
      eventRecord(
        this.state.tick,
        target.position,
        "Kill",
        `E(${attacker.id}) defeated E(${target.id})`,
        [attacker.id, target.id]
      )
    );
    attacker.combatTargetId = null;

    if (target.role === "player") {
      target.health = target.maxHealth;
      target.position = [...target.spawn];
      target.combatTargetId = null;
      target.desiredMove = null;
      events.push(
        eventRecord(
          this.state.tick,
          target.position,
          "EntitySpawned",
          `Player E(${target.id}) respawned`,
          [target.id]
        )
      );
      return;
    }

    if (target.role === "wild") {
      this.spawnLootFromCreature(target);
    }

    this.destroyEntity(target.id);
  }

  private spawnLootFromCreature(target: LocalEntity): void {
    const loot = createLootEntity(
      this.state.nextEntityId,
      `${target.metadata.speciesName ?? target.label} Remains`,
      target.position,
      18,
      "beast-bone"
    );
    this.state.nextEntityId += 1;
    this.state.entities.push(loot);
  }

  private spawnCompanion(
    companion: CompanionRosterEntry,
    slot: number,
    playerPosition: Vec2Tuple
  ): LocalEntity {
    const entity = createCompanionEntity(
      this.state.nextEntityId,
      companion.speciesName,
      [playerPosition[0] - 1.1, playerPosition[1] + 0.9],
      slot,
      companion.speciesId
    );
    this.state.nextEntityId += 1;
    this.state.entities.push(entity);
    return entity;
  }

  private destroyEntity(entityId: number): void {
    this.state.entities = this.state.entities.filter((entity) => entity.id !== entityId);
  }

  private findEntity(entityId: number): LocalEntity | null {
    return this.state.entities.find((entity) => entity.id === entityId) ?? null;
  }
}

export function renderGameToText(
  snapshot: NetworkWorldSnapshot,
  controlledEntity: number | null,
  selectedTargetId: number | null,
  actionState: DirectConnectActionState,
  feedback: string,
  recentEvents: NetworkGameEvent[],
  companionRoster: CompanionRosterEntry[]
): string {
  const player = snapshot.entities.find((entity) => entity.id === controlledEntity) ?? null;
  const target = snapshot.entities.find((entity) => entity.id === selectedTargetId) ?? null;

  return JSON.stringify({
    mode: "local-sandbox",
    world: LOCAL_WORLD_NAME,
    coordinateSystem: "world x east-west, y north-south",
    tick: snapshot.tick,
    player: player
      ? {
          id: player.id,
          label: player.label,
          position: player.position,
          velocity: player.velocity,
          health: player.health,
          maxHealth: player.maxHealth
        }
      : null,
    target: target
      ? {
          id: target.id,
          label: target.label,
          kind: target.metadata.kind,
          position: target.position,
          health: target.health,
          maxHealth: target.maxHealth
        }
      : null,
    companions: companionRoster,
    actionState,
    feedback,
    events: recentEvents.slice(-4).map((event) => event.summary),
    nearby: snapshot.entities
      .filter((entity) => entity.id !== controlledEntity && entity.metadata.kind !== "Scenery")
      .sort((left, right) => {
        if (!player) {
          return left.id - right.id;
        }
        return distanceBetween(left.position, player.position) - distanceBetween(right.position, player.position);
      })
      .slice(0, 8)
      .map((entity) => ({
        id: entity.id,
        label: entity.label,
        kind: entity.metadata.kind,
        position: entity.position,
        health: entity.health,
        maxHealth: entity.maxHealth
      }))
  });
}

function createInitialState(playerName: string): LocalWorldState {
  const entities: LocalEntity[] = [
    createPlayerEntity(playerName),
    createNpcEntity(2, "Archivist Mara", [-2.1, -2.9]),
    createNpcEntity(8, "Forgekeeper Ivo", [2.5, -2.4]),
    createNpcEntity(9, "Warden Selene", [0.8, 3.3]),
    createWildCreatureEntity(3, "Verdant Lynx", [5.8, 1.9], 18, 32),
    createWildCreatureEntity(4, "Cinder Hare", [7.2, -5.8], 22, 30),
    createWildCreatureEntity(10, "Rift Stag", [1.8, 7.2], 26, 36),
    createResourceEntity(5, "Copper Vein", [3.8, -1.4], "Mining", "copper-ore"),
    createResourceEntity(6, "Ancient Pine", [-6.3, 4.8], "Woodcutting", "pine-log"),
    createResourceEntity(11, "Moonstone Outcrop", [6.4, 5.8], "Mining", "moonstone-shard"),
    createResourceEntity(12, "Silver Birch", [-7.4, 1.8], "Woodcutting", "birch-log"),
    createLootEntity(7, "Supply Cache", [-2.5, 1.8], 48, "travel-ration"),
    createLootEntity(13, "Expedition Chest", [4.8, 7.4], 96, "ember-charm"),
    createSceneryEntity(20, "wall north", [0, -10.8], [0, 0]),
    createSceneryEntity(21, "wall south", [0, 10.8], [0, 0]),
    createSceneryEntity(22, "wall west", [-11.2, 0], [0, 0]),
    createSceneryEntity(23, "wall east", [11.2, 0], [0, 0]),
    createSceneryEntity(24, "weathered boulder", [5.8, 5.6], [0, 0]),
    createSceneryEntity(25, "weathered boulder", [-5.4, -6.1], [0, 0]),
    createSceneryEntity(26, "glass spire", [0.1, 6.0], [0, 0]),
    createSceneryEntity(27, "canopy tree", [-4.6, 5.7], [0, 0]),
    createSceneryEntity(28, "canopy tree", [-7.6, 3.8], [0, 0]),
    createSceneryEntity(29, "basalt pillar", [4.5, -8.1], [0, 0]),
    createSceneryEntity(30, "basalt pillar", [6.3, -8.1], [0, 0]),
    createSceneryEntity(31, "weathered boulder", [8.6, 1.4], [0, 0]),
    createSceneryEntity(32, "weathered boulder", [8.4, 4.4], [0, 0]),
    createSceneryEntity(33, "canopy tree", [9.1, -2.1], [0, 0]),
    createSceneryEntity(34, "glass spire", [-8.4, -4.8], [0, 0]),
    createSceneryEntity(35, "basalt pillar", [-3.2, 8.8], [0, 0]),
    createSceneryEntity(36, "basalt pillar", [3.2, 8.8], [0, 0]),
    createSceneryEntity(37, "weathered boulder", [-8.8, -0.6], [0, 0]),
    createSceneryEntity(38, "canopy tree", [-9.0, -3.2], [0, 0]),
    createSceneryEntity(39, "wall shrine", [0, -8.7], [0, 0])
  ];

  return {
    tick: 0,
    entities,
    nextEntityId: 60,
    companionRoster: [],
    activeCompanionId: null,
    autoRetaliate: true
  };
}

function createPlayerEntity(playerName: string): LocalEntity {
  return {
    id: PLAYER_ID,
    role: "player",
    label: playerName,
    position: [0, 0],
    velocity: [0, 0],
    rotation: 0.4,
    movementSpeed: 4.8,
    health: 42,
    maxHealth: 42,
    metadata: metadata("Player", {
      teamId: 1,
      combatStyle: "Melee",
      interaction: interactionHints({
        canInspect: true,
        canInteract: true,
        canAttack: true,
        canChat: true
      })
    }),
    spawn: [0, 0],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.4,
    resourceRemaining: 0,
    lootCoins: 0,
    lootItemId: null,
    companionSlot: null,
    companionMode: null,
    anchor: null
  };
}

function createNpcEntity(id: number, label: string, position: Vec2Tuple): LocalEntity {
  return {
    id,
    role: "npc",
    label,
    position,
    velocity: [0, 0],
    rotation: 0,
    movementSpeed: 0,
    health: 28,
    maxHealth: 28,
    metadata: metadata("Npc", {
      teamId: 2,
      combatStyle: "Magic",
      interaction: interactionHints({
        canInspect: true,
        canInteract: true,
        canChat: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.4,
    resourceRemaining: 0,
    lootCoins: 0,
    lootItemId: null,
    companionSlot: null,
    companionMode: null,
    anchor: [...position]
  };
}

function createWildCreatureEntity(
  id: number,
  speciesName: string,
  position: Vec2Tuple,
  health: number,
  maxHealth: number
): LocalEntity {
  return {
    id,
    role: "wild",
    label: speciesName,
    position,
    velocity: [0, 0],
    rotation: 0,
    movementSpeed: 3.1,
    health,
    maxHealth,
    metadata: metadata("WildCreature", {
      combatStyle: "Melee",
      speciesId: slug(speciesName),
      speciesName,
      encounterKind: "WildCreature",
      interaction: interactionHints({
        canInspect: true,
        canAttack: true,
        canCapture: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.8,
    resourceRemaining: 0,
    lootCoins: 0,
    lootItemId: null,
    companionSlot: null,
    companionMode: null,
    anchor: [...position]
  };
}

function createResourceEntity(
  id: number,
  label: string,
  position: Vec2Tuple,
  skill: NetworkSkillKind,
  itemId: string
): LocalEntity {
  return {
    id,
    role: "resource",
    label,
    position,
    velocity: [0, 0],
    rotation: 0,
    movementSpeed: 0,
    health: null,
    maxHealth: null,
    metadata: metadata("ResourceNode", {
      resourceSkill: skill,
      resourceTier: 1,
      interaction: interactionHints({
        canInspect: true,
        canGather: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.5,
    resourceRemaining: 3,
    lootCoins: 0,
    lootItemId: itemId,
    companionSlot: null,
    companionMode: null,
    anchor: [...position]
  };
}

function createLootEntity(
  id: number,
  label: string,
  position: Vec2Tuple,
  coins: number,
  itemId: string
): LocalEntity {
  return {
    id,
    role: "loot",
    label,
    position,
    velocity: [0, 0],
    rotation: 0,
    movementSpeed: 0,
    health: null,
    maxHealth: null,
    metadata: metadata("LootContainer", {
      interaction: interactionHints({
        canInspect: true,
        canLoot: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.4,
    resourceRemaining: 0,
    lootCoins: coins,
    lootItemId: itemId,
    companionSlot: null,
    companionMode: null,
    anchor: [...position]
  };
}

function createCompanionEntity(
  id: number,
  speciesName: string,
  position: Vec2Tuple,
  slot: number,
  speciesId: string
): LocalEntity {
  return {
    id,
    role: "companion",
    label: speciesName,
    position,
    velocity: [0, 0],
    rotation: 0,
    movementSpeed: 4.2,
    health: 24,
    maxHealth: 24,
    metadata: metadata("Companion", {
      teamId: 1,
      combatStyle: "Summoning",
      speciesId,
      speciesName,
      interaction: interactionHints({
        canInspect: true,
        canCommandCompanion: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.6,
    resourceRemaining: 0,
    lootCoins: 0,
    lootItemId: null,
    companionSlot: slot,
    companionMode: "Follow",
    anchor: null
  };
}

function createSceneryEntity(
  id: number,
  label: string,
  position: Vec2Tuple,
  velocity: Vec2Tuple
): LocalEntity {
  return {
    id,
    role: "scenery",
    label,
    position,
    velocity,
    rotation: 0,
    movementSpeed: 0,
    health: null,
    maxHealth: null,
    metadata: metadata("Scenery", {
      interaction: interactionHints({
        canInspect: true
      })
    }),
    spawn: [...position],
    desiredMove: null,
    combatTargetId: null,
    attackCooldownTicks: 0,
    interactionRadius: 2.4,
    resourceRemaining: 0,
    lootCoins: 0,
    lootItemId: null,
    companionSlot: null,
    companionMode: null,
    anchor: [...position]
  };
}

function metadata(
  kind: NetworkEntityKind,
  overrides: Partial<NetworkEntityMetadataSnapshot> = {}
): NetworkEntityMetadataSnapshot {
  return {
    kind,
    teamId: null,
    combatStyle: null,
    speciesId: null,
    speciesName: null,
    resourceSkill: null,
    resourceTier: null,
    encounterKind: null,
    interaction: interactionHints(),
    ...overrides
  };
}

function interactionHints(
  overrides: Partial<NetworkEntityInteractionHints> = {}
): NetworkEntityInteractionHints {
  return {
    canInspect: false,
    canInteract: false,
    canAttack: false,
    canGather: false,
    canLoot: false,
    canCapture: false,
    canCommandCompanion: false,
    canChat: false,
    ...overrides
  };
}

function createStatus(state: LocalWorldState): DirectConnectStatus {
  return {
    phase: "idle",
    detail: "Local sandbox shard idle",
    url: LOCAL_WORLD_URL,
    tick: state.tick,
    entityCount: state.entities.length,
    controlledEntity: PLAYER_ID,
    authoritativeDigest: state.tick
  };
}

function createActionState(): DirectConnectActionState {
  return {
    pendingCount: 0,
    lastSubmittedTick: null,
    lastAcknowledgedTick: null,
    lastRejectedTick: null,
    lastRejectedReason: null,
    lastActionSummary: null
  };
}

function toSnapshot(entity: LocalEntity): NetworkEntitySnapshot {
  return {
    id: entity.id,
    position: entity.position,
    velocity: entity.velocity,
    rotation: entity.rotation,
    health: entity.health,
    maxHealth: entity.maxHealth,
    movementSpeed: entity.movementSpeed,
    label: entity.label,
    metadata: entity.metadata
  };
}

function requireEntity(state: LocalWorldState, id: number): LocalEntity {
  const entity = state.entities.find((candidate) => candidate.id === id);
  if (!entity) {
    throw new Error(`Missing local sandbox entity ${id}`);
  }
  return entity;
}

function withinRange(a: Vec2Tuple, b: Vec2Tuple, radius: number): boolean {
  return distanceBetween(a, b) <= radius;
}

function distanceBetween(a: Vec2Tuple, b: Vec2Tuple): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1]);
}

function moveToward(a: Vec2Tuple, b: Vec2Tuple): Vec2Tuple | null {
  return normalize([b[0] - a[0], b[1] - a[1]]);
}

function normalize(vector: Vec2Tuple): Vec2Tuple | null {
  const length = Math.hypot(vector[0], vector[1]);
  if (length <= 0.0001) {
    return null;
  }
  return [vector[0] / length, vector[1] / length];
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function angleBetween(origin: Vec2Tuple, target: Vec2Tuple): number {
  return Math.atan2(target[0] - origin[0], target[1] - origin[1]);
}

function slug(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

function summarizeActions(actions: BrowserAction[]): string {
  return actions.map((action) => action.kind.replace(/([A-Z])/g, " $1").toLowerCase()).join(" + ");
}

function describeInteraction(target: LocalEntity): string {
  switch (target.role) {
    case "npc":
      return `${target.label} points you toward the copper vein and wild lynx`;
    case "resource":
      return `${target.label} hums with gatherable energy`;
    case "loot":
      return `${target.label} looks ready to open`;
    case "scenery":
      return `${target.label} anchors the edge of the test world`;
    default:
      return `${target.label} can be inspected`;
  }
}

function baseDamage(attacker: LocalEntity, tick: number, targetId: number): number {
  const base =
    attacker.role === "companion" ? 6 : attacker.role === "wild" ? 5 : attacker.role === "npc" ? 4 : 8;
  return base + ((tick + attacker.id + targetId) % 3);
}

function eventRecord(
  tick: number,
  origin: Vec2Tuple,
  kind: string,
  summary: string,
  entityIds: number[]
): NetworkGameEvent {
  return {
    tick,
    origin,
    kind,
    summary,
    entityIds
  };
}
