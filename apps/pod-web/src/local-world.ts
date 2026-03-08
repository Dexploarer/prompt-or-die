import type {
  BrowserAction,
  BrowserCompanionCommand,
  NetworkAtmosphereProfile,
  NetworkCombatStyle,
  NetworkEncounterKind,
  NetworkEntityInteractionHints,
  NetworkEntityKind,
  NetworkEntityMetadataSnapshot,
  NetworkEntitySnapshot,
  NetworkEventBatch,
  NetworkGameEvent,
  NetworkPopulationBreakdown,
  NetworkChunkPopulationState,
  NetworkRegionPopulationState,
  NetworkSkillKind,
  NetworkWorldPopulationState,
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
const WORLD_LIMIT = 28;
const LOCAL_WORLD_CHUNK_SIZE = 8;
const LOCAL_ACTIVE_CHUNK_RADIUS = 1;

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

interface LocalActionResult {
  rejection: string | null;
  progressionDirty: boolean;
}

interface LocalQuestStageDefinition {
  stageId: string;
  title: string;
  objectives: string[];
  stageTags: string[];
  nextStageId: string | null;
}

interface LocalQuestGraphState {
  graphId: string;
  displayName: string;
  repeatable: boolean;
  currentStageId: string;
  completedStageIds: string[];
  stages: LocalQuestStageDefinition[];
}

interface LocalFactionReputationTier {
  tierId: string;
  label: string;
  minimumScore: number;
  perkTags: string[];
}

interface LocalFactionTrackState {
  trackId: string;
  displayName: string;
  score: number;
  tiers: LocalFactionReputationTier[];
}

interface LocalEncounterSpawnEntry {
  archetypeId: string;
  weight: number;
  minCount: number;
  maxCount: number;
  requiredStageTags: string[];
  requiredReputationTiers: string[];
}

interface LocalEncounterTableState {
  tableId: string;
  biomeId: string;
  spawnGroup: string;
  ambientCap: number;
  entries: LocalEncounterSpawnEntry[];
}

interface LocalRegionState {
  regionId: string;
  displayName: string;
  primaryBiomeId: string;
  chunkKeys: string[];
  activeQuestGraphIds: string[];
  dominantFactionTrackId: string;
  encounterTableIds: string[];
}

interface LocalPersistedEntityRecord {
  entity: LocalEntity | null;
  removedAtTick: number | null;
  respawnTemplate: boolean;
  chunkKey: string;
}

export interface LocalWorldDebugState {
  activeChunkKeys: string[];
  currentRegionId: string | null;
  currentRegionName: string | null;
  questGraphs: Array<{
    graphId: string;
    displayName: string;
    currentStageId: string;
    currentStageTitle: string;
    currentStageTags: string[];
    completed: boolean;
  }>;
  factionReputation: Array<{
    trackId: string;
    displayName: string;
    score: number;
    tierId: string;
    tierLabel: string;
  }>;
  encounterTables: Array<{
    tableId: string;
    biomeId: string;
    spawnGroup: string;
    ambientCap: number;
  }>;
}

interface LocalWorldState {
  tick: number;
  entities: LocalEntity[];
  nextEntityId: number;
  companionRoster: CompanionRosterEntry[];
  activeCompanionId: number | null;
  autoRetaliate: boolean;
  activeChunkKeys: string[];
  templateEntities: Map<number, LocalEntity>;
  templateChunkEntityIds: Map<string, number[]>;
  persistedEntities: Map<number, LocalPersistedEntityRecord>;
  dynamicChunkEntityIds: Map<string, Set<number>>;
  regions: Map<string, LocalRegionState>;
  questGraphs: Map<string, LocalQuestGraphState>;
  factionTracks: Map<string, LocalFactionTrackState>;
  encounterTables: Map<string, LocalEncounterTableState>;
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
    const entities = this.state.entities
      .map((entity) => toSnapshot(entity))
      .sort((left, right) => left.id - right.id);

    return {
      tick: this.state.tick,
      entities,
      population: summarizePopulationState(
        this.state.tick,
        entities,
        this.state.regions,
        this.state.encounterTables
      )
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

  currentDebugState(): LocalWorldDebugState {
    const player = this.findEntity(PLAYER_ID);
    const currentRegionId = player?.metadata.regionId ?? null;
    const currentRegionName = player?.metadata.regionName ?? null;

    return {
      activeChunkKeys: this.state.activeChunkKeys.slice(),
      currentRegionId,
      currentRegionName,
      questGraphs: Array.from(this.state.questGraphs.values()).map((graph) => {
        const stage = graph.stages.find((entry) => entry.stageId === graph.currentStageId);
        return {
          graphId: graph.graphId,
          displayName: graph.displayName,
          currentStageId: graph.currentStageId,
          currentStageTitle: stage?.title ?? graph.currentStageId,
          currentStageTags: stage?.stageTags ?? [],
          completed: graph.completedStageIds.includes(graph.currentStageId) && stage?.nextStageId == null
        };
      }),
      factionReputation: Array.from(this.state.factionTracks.values()).map((track) => {
        const tier = currentFactionTier(track);
        return {
          trackId: track.trackId,
          displayName: track.displayName,
          score: track.score,
          tierId: tier.tierId,
          tierLabel: tier.label
        };
      }),
      encounterTables: Array.from(this.state.encounterTables.values()).map((table) => ({
        tableId: table.tableId,
        biomeId: table.biomeId,
        spawnGroup: table.spawnGroup,
        ambientCap: table.ambientCap
      }))
    };
  }

  private runTick(): void {
    this.state.tick += 1;
    const events: NetworkGameEvent[] = [];
    let progressionDirty = false;

    progressionDirty = this.processPendingActions(events) || progressionDirty;
    this.updateAutonomousBehaviors();
    this.integrateMovement();
    this.syncStreamingMetadataForActiveEntities();
    this.resolveCombat(events);
    this.updateChunkResidency(progressionDirty);

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

  private processPendingActions(events: NetworkGameEvent[]): boolean {
    const ready = this.pendingActions.filter((batch) => batch.tick <= this.state.tick);
    this.pendingActions = this.pendingActions.filter((batch) => batch.tick > this.state.tick);
    let progressionDirty = false;

    for (const batch of ready) {
      let rejectedReason: string | null = null;
      for (const action of batch.actions) {
        const outcome = this.applyAction(action, events);
        const rejection = outcome.rejection;
        progressionDirty = progressionDirty || outcome.progressionDirty;
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

    return progressionDirty;
  }

  private applyAction(action: BrowserAction, events: NetworkGameEvent[]): LocalActionResult {
    const player = requireEntity(this.state, PLAYER_ID);

    switch (action.kind) {
      case "move":
        player.desiredMove = normalize(action.direction);
        return accepted();
      case "stop":
        player.desiredMove = null;
        player.combatTargetId = null;
        return accepted();
      case "rotate":
        player.rotation = action.angle;
        return accepted();
      case "lookAt":
        player.rotation = angleBetween(player.position, action.target);
        return accepted();
      case "attack":
        return rejected("Select a target before attacking");
      case "attackTarget": {
        const target = this.findEntity(action.target);
        if (!target || !target.metadata.interaction.canAttack) {
          return rejected("Target cannot be attacked");
        }
        if (!withinRange(player.position, target.position, MELEE_RANGE)) {
          return rejected("Move closer to attack");
        }
        player.combatTargetId = target.id;
        if (target.role === "wild") {
          target.combatTargetId = player.id;
        }
        this.performAttack(player, target, events, MELEE_RANGE, PLAYER_ATTACK_COOLDOWN);
        return accepted();
      }
      case "interact":
        return rejected("Select a target before interacting");
      case "interactWith": {
        const target = this.findEntity(action.target);
        if (!target || !target.metadata.interaction.canInspect) {
          return rejected("Target cannot be inspected");
        }
        if (!withinRange(player.position, target.position, target.interactionRadius + 0.8)) {
          return rejected("Move closer to inspect");
        }
        events.push(eventRecord(this.state.tick, player.position, "Interact", describeInteraction(target), [
          player.id,
          target.id
        ]));
        return this.handleInteractionProgression(player, target, events);
      }
      case "gatherResource": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "resource" || !target.metadata.interaction.canGather) {
          return rejected("Target cannot be gathered");
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return rejected("Move closer to gather");
        }
        target.resourceRemaining = Math.max(0, target.resourceRemaining - 1);
        const depleted = target.resourceRemaining === 0;
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
        if (depleted) {
          this.destroyEntity(target.id);
        }
        this.adjustFactionReputation("verdant-wardens", 2, target.position, "Gather support", events);
        return accepted(true);
      }
      case "loot": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "loot" || !target.metadata.interaction.canLoot) {
          return rejected("Target has no loot");
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return rejected("Move closer to loot");
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
        if (target.label === "Expedition Chest") {
          this.advanceQuestGraph("ember-charm-recovery", target.position, events);
          this.adjustFactionReputation("ancient-spirekeepers", 4, target.position, "Recovered relic cache", events);
        }
        this.destroyEntity(target.id);
        return accepted(target.label === "Expedition Chest");
      }
      case "captureCreature": {
        const target = this.findEntity(action.target);
        if (!target || target.role !== "wild" || !target.metadata.interaction.canCapture) {
          return rejected("Target cannot be captured");
        }
        if (!withinRange(player.position, target.position, target.interactionRadius)) {
          return rejected("Move closer to capture");
        }
        if ((target.health ?? 0) > (target.maxHealth ?? 0) * 0.5) {
          return rejected("Weaken the creature first");
        }
        if (this.state.companionRoster.length >= 3) {
          return rejected("Companion roster is full");
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
        this.advanceQuestGraph("lynx-patrol", target.position, events);
        this.adjustFactionReputation("verdant-wardens", 6, target.position, "Captured hostile wildlife", events);
        this.adjustFactionReputation("verdant-wilds", -4, target.position, "Captured apex creature", events);
        this.destroyEntity(target.id);
        player.combatTargetId = null;
        return accepted(true);
      }
      case "summonCompanion": {
        const companion = this.state.companionRoster[action.slot];
        if (!companion) {
          return rejected("No companion in that slot");
        }
        if (this.state.activeCompanionId != null && this.findEntity(this.state.activeCompanionId)) {
          return rejected("A companion is already active");
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
        return accepted();
      }
      case "commandCompanion": {
        const companion = this.state.activeCompanionId != null
          ? this.findEntity(this.state.activeCompanionId)
          : null;
        if (!companion || companion.role !== "companion") {
          return rejected("No active companion");
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
          return accepted();
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
        return accepted();
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
        return accepted();
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
        return accepted();
      case "idle":
        return accepted();
    }
  }

  private handleInteractionProgression(
    player: LocalEntity,
    target: LocalEntity,
    events: NetworkGameEvent[]
  ): LocalActionResult {
    let progressionDirty = false;

    switch (target.label) {
      case "Archivist Mara":
        progressionDirty = this.advanceQuestGraph("verdant-intro", target.position, events) || progressionDirty;
        break;
      case "Forgekeeper Ivo":
        progressionDirty = this.advanceQuestGraph("tempered-trail", target.position, events) || progressionDirty;
        break;
      case "Warden Selene":
        progressionDirty = this.advanceQuestGraph("lynx-patrol", target.position, events) || progressionDirty;
        break;
      case "glass spire":
        progressionDirty = this.advanceQuestGraph("verdant-intro", target.position, events) || progressionDirty;
        progressionDirty = this.advanceQuestGraph("spire-attunement", target.position, events) || progressionDirty;
        progressionDirty =
          this.adjustFactionReputation(
            "ancient-spirekeepers",
            6,
            target.position,
            "Attuned the glass spire",
            events
          ) || progressionDirty;
        break;
      default:
        break;
    }

    if (target.metadata.factionTrackId) {
      progressionDirty =
        this.adjustFactionReputation(
          target.metadata.factionTrackId,
          target.label === "glass spire" ? 0 : 1,
          player.position,
          `Interacted with ${target.label}`,
          events
        ) || progressionDirty;
    }

    return accepted(progressionDirty);
  }

  private advanceQuestGraph(
    graphId: string,
    origin: Vec2Tuple,
    events: NetworkGameEvent[]
  ): boolean {
    const graph = this.state.questGraphs.get(graphId);
    if (!graph) {
      return false;
    }

    const currentStage = graph.stages.find((stage) => stage.stageId === graph.currentStageId);
    if (!currentStage) {
      return false;
    }

    if (!graph.completedStageIds.includes(currentStage.stageId)) {
      graph.completedStageIds.push(currentStage.stageId);
    }

    if (currentStage.nextStageId == null) {
      if (!graph.repeatable) {
        events.push(
          eventRecord(
            this.state.tick,
            origin,
            "QuestCompleted",
            `${graph.displayName} completed`,
            [PLAYER_ID]
          )
        );
        return true;
      }
      graph.currentStageId = graph.stages[0]?.stageId ?? currentStage.stageId;
      events.push(
        eventRecord(
          this.state.tick,
          origin,
          "QuestReset",
          `${graph.displayName} reset`,
          [PLAYER_ID]
        )
      );
      return true;
    }

    graph.currentStageId = currentStage.nextStageId;
    const nextStage = graph.stages.find((stage) => stage.stageId === graph.currentStageId);
    events.push(
      eventRecord(
        this.state.tick,
        origin,
        "QuestAdvanced",
        `${graph.displayName} -> ${nextStage?.title ?? graph.currentStageId}`,
        [PLAYER_ID]
      )
    );
    return true;
  }

  private adjustFactionReputation(
    trackId: string,
    delta: number,
    origin: Vec2Tuple,
    reason: string,
    events: NetworkGameEvent[]
  ): boolean {
    if (delta === 0) {
      return false;
    }

    const track = this.state.factionTracks.get(trackId);
    if (!track) {
      return false;
    }

    const previousTier = currentFactionTier(track);
    track.score += delta;
    const nextTier = currentFactionTier(track);
    events.push(
      eventRecord(
        this.state.tick,
        origin,
        "FactionReputationChanged",
        `${track.displayName} ${delta > 0 ? "+" : ""}${delta} (${reason})`,
        [PLAYER_ID]
      )
    );

    if (previousTier.tierId !== nextTier.tierId) {
      events.push(
        eventRecord(
          this.state.tick,
          origin,
          "FactionTierChanged",
          `${track.displayName} -> ${nextTier.label}`,
          [PLAYER_ID]
        )
      );
    }

    return true;
  }

  private syncStreamingMetadataForActiveEntities(): void {
    for (const entity of this.state.entities) {
      syncStreamingMetadataForEntity(entity, this.state);
    }
  }

  private updateChunkResidency(forceRefresh: boolean): void {
    const player = requireEntity(this.state, PLAYER_ID);
    const desiredChunks = expandDesiredChunkKeys(
      chunkKeyFromPosition(player.position),
      LOCAL_ACTIVE_CHUNK_RADIUS
    );
    const currentChunks = new Set(this.state.activeChunkKeys);

    for (const chunkKey of this.state.activeChunkKeys) {
      if (!desiredChunks.includes(chunkKey)) {
        this.deactivateChunk(chunkKey);
      }
    }

    for (const chunkKey of desiredChunks) {
      if (!currentChunks.has(chunkKey)) {
        this.activateChunk(chunkKey);
      } else if (forceRefresh) {
        this.reconcileChunk(chunkKey);
      }
    }

    this.state.activeChunkKeys = desiredChunks;
  }

  private deactivateChunk(chunkKey: string): void {
    const removedIds = new Set<number>();
    for (const entity of this.state.entities) {
      if (
        entity.id === PLAYER_ID ||
        entity.role === "companion" ||
        entity.metadata.chunkKey !== chunkKey
      ) {
        continue;
      }

      this.state.persistedEntities.set(entity.id, {
        entity: cloneEntity(entity),
        removedAtTick: null,
        respawnTemplate: this.state.templateEntities.has(entity.id),
        chunkKey
      });
      removedIds.add(entity.id);
    }

    if (removedIds.size === 0) {
      return;
    }

    const player = requireEntity(this.state, PLAYER_ID);
    if (player.combatTargetId != null && removedIds.has(player.combatTargetId)) {
      player.combatTargetId = null;
    }

    const companion =
      this.state.activeCompanionId != null ? this.findEntity(this.state.activeCompanionId) : null;
    if (companion?.combatTargetId != null && removedIds.has(companion.combatTargetId)) {
      companion.combatTargetId = null;
    }

    this.state.entities = this.state.entities.filter((entity) => !removedIds.has(entity.id));
  }

  private activateChunk(chunkKey: string): void {
    const templateIds = this.state.templateChunkEntityIds.get(chunkKey) ?? [];
    for (const entityId of templateIds) {
      if (this.findEntity(entityId)) {
        continue;
      }
      const template = this.state.templateEntities.get(entityId);
      if (!template || !shouldTemplateEntityBeActive(template, this.state)) {
        continue;
      }
      const entity = this.instantiateChunkEntity(entityId, chunkKey);
      if (entity) {
        this.state.entities.push(entity);
      }
    }

    const dynamicIds = this.state.dynamicChunkEntityIds.get(chunkKey);
    if (!dynamicIds) {
      return;
    }
    for (const entityId of dynamicIds) {
      if (this.findEntity(entityId) || this.state.templateEntities.has(entityId)) {
        continue;
      }
      const record = this.state.persistedEntities.get(entityId);
      if (record?.entity) {
        const entity = cloneEntity(record.entity);
        syncStreamingMetadataForEntity(entity, this.state);
        this.state.entities.push(entity);
      }
    }
  }

  private reconcileChunk(chunkKey: string): void {
    const templateIds = this.state.templateChunkEntityIds.get(chunkKey) ?? [];

    for (const entityId of templateIds) {
      const activeEntity = this.findEntity(entityId);
      const template = this.state.templateEntities.get(entityId);
      if (!template) {
        continue;
      }

      const shouldBeActive = shouldTemplateEntityBeActive(template, this.state);
      if (activeEntity && !shouldBeActive) {
        this.deactivateChunkEntity(activeEntity, chunkKey);
      } else if (!activeEntity && shouldBeActive) {
        const entity = this.instantiateChunkEntity(entityId, chunkKey);
        if (entity) {
          this.state.entities.push(entity);
        }
      }
    }
  }

  private deactivateChunkEntity(entity: LocalEntity, chunkKey: string): void {
    this.state.persistedEntities.set(entity.id, {
      entity: cloneEntity(entity),
      removedAtTick: null,
      respawnTemplate: this.state.templateEntities.has(entity.id),
      chunkKey
    });
    this.state.entities = this.state.entities.filter((candidate) => candidate.id !== entity.id);
  }

  private instantiateChunkEntity(entityId: number, chunkKey: string): LocalEntity | null {
    const template = this.state.templateEntities.get(entityId);
    if (!template) {
      return null;
    }

    const record = this.state.persistedEntities.get(entityId);
    if (record) {
      if (record.entity) {
        const entity = cloneEntity(record.entity);
        syncStreamingMetadataForEntity(entity, this.state);
        return entity;
      }

      if (
        !record.respawnTemplate ||
        record.removedAtTick == null ||
        !canRespawnTemplate(template, record.removedAtTick, this.state.tick)
      ) {
        return null;
      }
    }

    const entity = cloneEntity(template);
    syncStreamingMetadataForEntity(entity, this.state);
    this.state.persistedEntities.delete(entityId);
    return entity;
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
      const targetVelocity: Vec2Tuple = desired
        ? [desired[0] * entity.movementSpeed, desired[1] * entity.movementSpeed]
        : [0, 0];
      const acceleration =
        entity.role === "player"
          ? entity.movementSpeed * 0.18
          : entity.movementSpeed * 0.14;
      const deceleration =
        entity.role === "player"
          ? entity.movementSpeed * 0.24
          : entity.movementSpeed * 0.18;
      const velocityStep =
        desired != null
          ? Math.max(0.08, acceleration)
          : Math.max(0.08, deceleration);

      entity.velocity = [
        moveScalarToward(entity.velocity[0], targetVelocity[0], velocityStep),
        moveScalarToward(entity.velocity[1], targetVelocity[1], velocityStep)
      ];

      const speed = Math.hypot(entity.velocity[0], entity.velocity[1]);
      if (speed < 0.02) {
        entity.velocity = [0, 0];
      }

      entity.position = [
        clamp(entity.position[0] + entity.velocity[0] / 60, -WORLD_LIMIT, WORLD_LIMIT),
        clamp(entity.position[1] + entity.velocity[1] / 60, -WORLD_LIMIT, WORLD_LIMIT)
      ];

      if (speed > 0) {
        entity.rotation = rotateTowardAngle(
          entity.rotation,
          Math.atan2(entity.velocity[0], entity.velocity[1]),
          0.24
        );
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
    syncStreamingMetadataForEntity(loot, this.state);
    const chunkKey = loot.metadata.chunkKey ?? chunkKeyFromPosition(loot.position);
    let dynamicIds = this.state.dynamicChunkEntityIds.get(chunkKey);
    if (!dynamicIds) {
      dynamicIds = new Set<number>();
      this.state.dynamicChunkEntityIds.set(chunkKey, dynamicIds);
    }
    dynamicIds.add(loot.id);
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
    syncStreamingMetadataForEntity(entity, this.state);
    this.state.entities.push(entity);
    return entity;
  }

  private destroyEntity(entityId: number): void {
    const entity = this.findEntity(entityId);
    if (!entity) {
      return;
    }

    const chunkKey = entity.metadata.chunkKey ?? chunkKeyFromPosition(entity.position);
    if (entity.role !== "player" && entity.role !== "companion") {
      this.state.persistedEntities.set(entity.id, {
        entity: null,
        removedAtTick: this.state.tick,
        respawnTemplate: this.state.templateEntities.has(entity.id),
        chunkKey
      });
    }

    this.state.entities = this.state.entities.filter((candidate) => candidate.id !== entityId);
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
  companionRoster: CompanionRosterEntry[],
  debugState: LocalWorldDebugState
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
    streaming: {
      chunkSize: LOCAL_WORLD_CHUNK_SIZE,
      activeChunks: debugState.activeChunkKeys,
      currentRegionId: debugState.currentRegionId,
      currentRegionName: debugState.currentRegionName,
      regionPopulation:
        player?.metadata.regionId == null
          ? null
          : snapshot.population.regions.find(
              (region) => region.regionId === player.metadata.regionId
            ) ?? null
    },
    progression: {
      questGraphs: debugState.questGraphs,
      factionReputation: debugState.factionReputation,
      encounterTables: debugState.encounterTables
    },
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
  const player = createPlayerEntity(playerName);
  const regions = createRegionCatalog();
  const questGraphs = createQuestGraphCatalog();
  const factionTracks = createFactionTrackCatalog();
  const encounterTables = createEncounterTableCatalog();
  const templates = authoredTemplateEntities();
  const templateEntities = new Map<number, LocalEntity>();
  const templateChunkEntityIds = new Map<string, number[]>();

  const draftState: LocalWorldState = {
    tick: 0,
    entities: [player],
    nextEntityId: 60,
    companionRoster: [],
    activeCompanionId: null,
    autoRetaliate: true,
    activeChunkKeys: [],
    templateEntities,
    templateChunkEntityIds,
    persistedEntities: new Map<number, LocalPersistedEntityRecord>(),
    dynamicChunkEntityIds: new Map<string, Set<number>>(),
    regions,
    questGraphs,
    factionTracks,
    encounterTables
  };

  syncStreamingMetadataForEntity(player, draftState);

  for (const template of templates) {
    syncStreamingMetadataForEntity(template, draftState);
    templateEntities.set(template.id, template);
    const chunkKey = template.metadata.chunkKey ?? chunkKeyFromPosition(template.position);
    const chunkEntityIds = templateChunkEntityIds.get(chunkKey) ?? [];
    chunkEntityIds.push(template.id);
    templateChunkEntityIds.set(chunkKey, chunkEntityIds);
  }

  draftState.activeChunkKeys = expandDesiredChunkKeys(
    chunkKeyFromPosition(player.position),
    LOCAL_ACTIVE_CHUNK_RADIUS
  );

  for (const chunkKey of draftState.activeChunkKeys) {
    const ids = templateChunkEntityIds.get(chunkKey) ?? [];
    for (const entityId of ids) {
      const template = templateEntities.get(entityId);
      if (!template || !shouldTemplateEntityBeActive(template, draftState)) {
        continue;
      }
      draftState.entities.push(cloneEntity(template));
    }
  }

  return draftState;
}

function authoredTemplateEntities(): LocalEntity[] {
  return [
    createNpcEntity(2, "Archivist Mara", [-5.4, -5.2]),
    createNpcEntity(8, "Forgekeeper Ivo", [7.8, -5.4]),
    createNpcEntity(9, "Warden Selene", [5.4, 5.8]),
    createWildCreatureEntity(3, "Verdant Lynx", [13.6, 4.2], 18, 32),
    createWildCreatureEntity(4, "Cinder Hare", [13.8, -9.4], 22, 30),
    createWildCreatureEntity(10, "Rift Stag", [-6.8, 15.8], 26, 36),
    createResourceEntity(5, "Copper Vein", [9.2, -2.6], "Mining", "copper-ore"),
    createResourceEntity(6, "Ancient Pine", [-12.4, 8.6], "Woodcutting", "pine-log"),
    createResourceEntity(11, "Moonstone Outcrop", [7.4, 14.8], "Mining", "moonstone-shard"),
    createResourceEntity(12, "Silver Birch", [-13.6, 2.8], "Woodcutting", "birch-log"),
    createLootEntity(7, "Supply Cache", [1.2, -7.8], 48, "travel-ration"),
    createLootEntity(13, "Expedition Chest", [8.8, 16.0], 96, "ember-charm"),
    createSceneryEntity(20, "wall north", [0, -16.4], [0, 0]),
    createSceneryEntity(21, "wall south", [0, 16.4], [0, 0]),
    createSceneryEntity(22, "wall west", [-16.8, 0], [0, 0]),
    createSceneryEntity(23, "wall east", [16.8, 0], [0, 0]),
    createSceneryEntity(24, "weathered boulder", [8.8, 7.8], [0, 0]),
    createSceneryEntity(25, "weathered boulder", [-7.8, -9.0], [0, 0]),
    createSceneryEntity(26, "glass spire", [0.8, 13.8], [0, 0]),
    createSceneryEntity(27, "canopy tree", [-6.2, 9.4], [0, 0]),
    createSceneryEntity(28, "canopy tree", [-10.8, 5.4], [0, 0]),
    createSceneryEntity(29, "basalt pillar", [7.8, -12.4], [0, 0]),
    createSceneryEntity(30, "basalt pillar", [10.6, -12.6], [0, 0]),
    createSceneryEntity(31, "weathered boulder", [12.4, 1.6], [0, 0]),
    createSceneryEntity(32, "weathered boulder", [15.8, 7.4], [0, 0]),
    createSceneryEntity(33, "canopy tree", [12.8, -3.6], [0, 0]),
    createSceneryEntity(34, "glass spire", [-12.8, -7.2], [0, 0]),
    createSceneryEntity(35, "basalt pillar", [-4.6, 14.2], [0, 0]),
    createSceneryEntity(36, "basalt pillar", [2.8, 15.0], [0, 0]),
    createSceneryEntity(37, "weathered boulder", [-13.2, -1.2], [0, 0]),
    createSceneryEntity(38, "canopy tree", [-13.6, -5.2], [0, 0]),
    createSceneryEntity(39, "wall shrine", [0, -13.2], [0, 0])
  ];
}

function createRegionCatalog(): Map<string, LocalRegionState> {
  return new Map<string, LocalRegionState>([
    [
      "verdant-heart",
      {
        regionId: "verdant-heart",
        displayName: "Verdant Heart",
        primaryBiomeId: "verdant-hollow",
        chunkKeys: ["-1:-1", "-1:0", "0:-1", "0:0"],
        activeQuestGraphIds: ["verdant-intro", "tempered-trail"],
        dominantFactionTrackId: "verdant-wardens",
        encounterTableIds: ["verdant-heart-wildlife", "verdant-heart-resources"]
      }
    ],
    [
      "spirewatch",
      {
        regionId: "spirewatch",
        displayName: "Spirewatch Rise",
        primaryBiomeId: "verdant-hollow",
        chunkKeys: ["-1:1", "0:1"],
        activeQuestGraphIds: ["spire-attunement", "ember-charm-recovery"],
        dominantFactionTrackId: "ancient-spirekeepers",
        encounterTableIds: ["spirewatch-encounters", "spirewatch-resources"]
      }
    ],
    [
      "ashen-steppe",
      {
        regionId: "ashen-steppe",
        displayName: "Ashen Steppe",
        primaryBiomeId: "ashen-steppe",
        chunkKeys: ["0:-2", "1:-1", "1:0"],
        activeQuestGraphIds: ["lynx-patrol"],
        dominantFactionTrackId: "verdant-wilds",
        encounterTableIds: ["ashen-steppe-encounters"]
      }
    ],
    [
      "gloamwood-edge",
      {
        regionId: "gloamwood-edge",
        displayName: "Gloamwood Edge",
        primaryBiomeId: "gloamwood",
        chunkKeys: ["-2:-1", "-2:0"],
        activeQuestGraphIds: ["lynx-patrol"],
        dominantFactionTrackId: "verdant-wilds",
        encounterTableIds: ["gloamwood-encounters", "gloamwood-resources"]
      }
    ]
  ]);
}

function createQuestGraphCatalog(): Map<string, LocalQuestGraphState> {
  return new Map<string, LocalQuestGraphState>([
    [
      "verdant-intro",
      {
        graphId: "verdant-intro",
        displayName: "Verdant Introduction",
        repeatable: false,
        currentStageId: "speak-to-mara",
        completedStageIds: [],
        stages: [
          {
            stageId: "speak-to-mara",
            title: "Speak to Archivist Mara",
            objectives: ["Report to Mara in the Verdant Heart"],
            stageTags: ["intro", "hub"],
            nextStageId: "attune-spire"
          },
          {
            stageId: "attune-spire",
            title: "Attune the Glass Spire",
            objectives: ["Inspect the glass spire in Spirewatch Rise"],
            stageTags: ["attunement"],
            nextStageId: "wardens-briefing"
          },
          {
            stageId: "wardens-briefing",
            title: "Return to the Wardens",
            objectives: ["Brief Warden Selene after the attunement"],
            stageTags: ["attuned", "patrol"],
            nextStageId: null
          }
        ]
      }
    ],
    [
      "lynx-patrol",
      {
        graphId: "lynx-patrol",
        displayName: "Lynx Patrol",
        repeatable: false,
        currentStageId: "report-to-selene",
        completedStageIds: [],
        stages: [
          {
            stageId: "report-to-selene",
            title: "Report to Warden Selene",
            objectives: ["Check in with Selene about patrol routes"],
            stageTags: ["patrol"],
            nextStageId: "capture-lynx"
          },
          {
            stageId: "capture-lynx",
            title: "Capture a Verdant Lynx",
            objectives: ["Weaken and capture a Verdant Lynx"],
            stageTags: ["hunt"],
            nextStageId: "return-to-selene"
          },
          {
            stageId: "return-to-selene",
            title: "Return with a Companion",
            objectives: ["Show Selene the captured companion"],
            stageTags: ["companions"],
            nextStageId: null
          }
        ]
      }
    ],
    [
      "tempered-trail",
      {
        graphId: "tempered-trail",
        displayName: "Tempered Trail",
        repeatable: false,
        currentStageId: "meet-ivo",
        completedStageIds: [],
        stages: [
          {
            stageId: "meet-ivo",
            title: "Meet Forgekeeper Ivo",
            objectives: ["Receive a supply list from Ivo"],
            stageTags: ["crafting"],
            nextStageId: "gather-copper"
          },
          {
            stageId: "gather-copper",
            title: "Gather Copper Ore",
            objectives: ["Mine copper from the Verdant Heart vein"],
            stageTags: ["gathering"],
            nextStageId: null
          }
        ]
      }
    ],
    [
      "spire-attunement",
      {
        graphId: "spire-attunement",
        displayName: "Spire Attunement",
        repeatable: false,
        currentStageId: "inspect-spire",
        completedStageIds: [],
        stages: [
          {
            stageId: "inspect-spire",
            title: "Inspect the Glass Spire",
            objectives: ["Reach the spire and attune to it"],
            stageTags: ["exploration"],
            nextStageId: "awaken-resonance"
          },
          {
            stageId: "awaken-resonance",
            title: "Awaken the Resonance",
            objectives: ["Unlock the moonstone outcrop"],
            stageTags: ["attuned", "resonance"],
            nextStageId: null
          }
        ]
      }
    ],
    [
      "ember-charm-recovery",
      {
        graphId: "ember-charm-recovery",
        displayName: "Ember Charm Recovery",
        repeatable: false,
        currentStageId: "search-expedition",
        completedStageIds: [],
        stages: [
          {
            stageId: "search-expedition",
            title: "Search the Expedition Grounds",
            objectives: ["Locate the expedition chest in Spirewatch Rise"],
            stageTags: ["ruins"],
            nextStageId: "recover-charm"
          },
          {
            stageId: "recover-charm",
            title: "Recover the Ember Charm",
            objectives: ["Loot the expedition chest"],
            stageTags: ["artifact"],
            nextStageId: null
          }
        ]
      }
    ]
  ]);
}

function createFactionTrackCatalog(): Map<string, LocalFactionTrackState> {
  return new Map<string, LocalFactionTrackState>([
    [
      "verdant-wardens",
      {
        trackId: "verdant-wardens",
        displayName: "Verdant Wardens",
        score: 18,
        tiers: [
          { tierId: "outsider", label: "Outsider", minimumScore: -99, perkTags: [] },
          { tierId: "ally", label: "Ally", minimumScore: 0, perkTags: ["field-support"] },
          { tierId: "trusted", label: "Trusted", minimumScore: 24, perkTags: ["supply-discounts"] },
          { tierId: "warden-sworn", label: "Warden Sworn", minimumScore: 48, perkTags: ["elite-patrols"] }
        ]
      }
    ],
    [
      "verdant-wilds",
      {
        trackId: "verdant-wilds",
        displayName: "Verdant Wilds",
        score: -8,
        tiers: [
          { tierId: "hostile", label: "Hostile", minimumScore: -99, perkTags: [] },
          { tierId: "watched", label: "Watched", minimumScore: -10, perkTags: ["reduced-aggro"] },
          { tierId: "calmed", label: "Calmed", minimumScore: 12, perkTags: ["rare-spawns"] }
        ]
      }
    ],
    [
      "ancient-spirekeepers",
      {
        trackId: "ancient-spirekeepers",
        displayName: "Ancient Spirekeepers",
        score: 0,
        tiers: [
          { tierId: "unknown", label: "Unknown", minimumScore: -99, perkTags: [] },
          { tierId: "noticed", label: "Noticed", minimumScore: 4, perkTags: ["spire-lore"] },
          { tierId: "resonant", label: "Resonant", minimumScore: 10, perkTags: ["moonstone-access"] }
        ]
      }
    ]
  ]);
}

function createEncounterTableCatalog(): Map<string, LocalEncounterTableState> {
  return new Map<string, LocalEncounterTableState>([
    [
      "verdant-heart-wildlife",
      {
        tableId: "verdant-heart-wildlife",
        biomeId: "verdant-hollow",
        spawnGroup: "wildlife",
        ambientCap: 4,
        entries: [
          {
            archetypeId: "verdant-lynx",
            weight: 8,
            minCount: 1,
            maxCount: 2,
            requiredStageTags: [],
            requiredReputationTiers: []
          }
        ]
      }
    ],
    [
      "verdant-heart-resources",
      {
        tableId: "verdant-heart-resources",
        biomeId: "verdant-hollow",
        spawnGroup: "resources",
        ambientCap: 4,
        entries: [
          {
            archetypeId: "copper-vein-resource",
            weight: 10,
            minCount: 1,
            maxCount: 1,
            requiredStageTags: [],
            requiredReputationTiers: []
          }
        ]
      }
    ],
    [
      "spirewatch-encounters",
      {
        tableId: "spirewatch-encounters",
        biomeId: "verdant-hollow",
        spawnGroup: "wildlife",
        ambientCap: 3,
        entries: [
          {
            archetypeId: "rift-stag",
            weight: 5,
            minCount: 1,
            maxCount: 1,
            requiredStageTags: ["patrol"],
            requiredReputationTiers: []
          }
        ]
      }
    ],
    [
      "spirewatch-resources",
      {
        tableId: "spirewatch-resources",
        biomeId: "verdant-hollow",
        spawnGroup: "resources",
        ambientCap: 2,
        entries: [
          {
            archetypeId: "moonstone-outcrop-resource",
            weight: 3,
            minCount: 1,
            maxCount: 1,
            requiredStageTags: ["attuned"],
            requiredReputationTiers: ["noticed"]
          }
        ]
      }
    ],
    [
      "ashen-steppe-encounters",
      {
        tableId: "ashen-steppe-encounters",
        biomeId: "ashen-steppe",
        spawnGroup: "wildlife",
        ambientCap: 3,
        entries: [
          {
            archetypeId: "cinder-hare",
            weight: 9,
            minCount: 1,
            maxCount: 2,
            requiredStageTags: [],
            requiredReputationTiers: []
          }
        ]
      }
    ],
    [
      "gloamwood-encounters",
      {
        tableId: "gloamwood-encounters",
        biomeId: "gloamwood",
        spawnGroup: "wildlife",
        ambientCap: 2,
        entries: []
      }
    ],
    [
      "gloamwood-resources",
      {
        tableId: "gloamwood-resources",
        biomeId: "gloamwood",
        spawnGroup: "resources",
        ambientCap: 3,
        entries: [
          {
            archetypeId: "silver-birch-resource",
            weight: 7,
            minCount: 1,
            maxCount: 1,
            requiredStageTags: [],
            requiredReputationTiers: []
          },
          {
            archetypeId: "ancient-pine-resource",
            weight: 5,
            minCount: 1,
            maxCount: 1,
            requiredStageTags: ["crafting"],
            requiredReputationTiers: []
          }
        ]
      }
    ]
  ]);
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
      faction: factionAffiliation("verdant-wardens", "initiate", "Friendly", 18),
      actorPresentation: {
        profileId: "hero-ranger",
        meshAssetId: "adventurer-hero",
        materialPaletteId: "verdant-hero",
        animationSetId: "humanoid-explorer",
        scaleMultiplier: 1.04,
        footprintRadius: 1.15,
        selectionRingScale: 2.8,
        auraColor: [0.18, 0.38, 0.52, 0.14]
      },
      combatPresentation: {
        profileId: "hero-combat",
        hitFlashColor: [0.96, 0.42, 0.26, 0.22],
        criticalRingColor: [0.96, 0.42, 0.26, 0.26],
        selectionRingColor: [0.62, 0.98, 0.84, 0.38],
        emissiveBoost: [0.08, 0.06, 0.02],
        impactScale: 1.12
      },
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
      faction: factionAffiliation("verdant-wardens", slug(label), "Friendly", 14),
      questAnchor:
        label === "Archivist Mara"
          ? questAnchor(
              ["verdant-intro", "spire-records"],
              "Ask Mara about the glass spire",
              ["intro", "lore"]
            )
          : label === "Forgekeeper Ivo"
            ? questAnchor(
                ["tempered-trail"],
                "Ask Ivo to prepare expedition gear",
                ["crafting", "gear"]
              )
            : questAnchor(
                ["lynx-patrol", "hollow-watch"],
                "Report the field state to Selene",
                ["combat", "patrol"]
              ),
      actorPresentation: {
        profileId: "hub-npc",
        meshAssetId: "adventurer-avatar",
        materialPaletteId: "archive-cloth",
        animationSetId: "humanoid-idle",
        scaleMultiplier: 1,
        footprintRadius: 1,
        selectionRingScale: 2.2,
        auraColor: [0, 0, 0, 0]
      },
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
      faction: factionAffiliation("verdant-wilds", "predator", "Hostile", 22),
      encounterProfile: encounterProfile(`${slug(speciesName)}-encounters`, 2, 1, 1_200),
      spawnProfile: spawnProfile(`${slug(speciesName)}-grove`, "verdant-hollow", "wildlife", 900, 16),
      actorPresentation: {
        profileId: slug(speciesName),
        meshAssetId: "rift-beast",
        materialPaletteId: "wild-creature",
        animationSetId: "beast-stalker",
        scaleMultiplier: 1.08,
        footprintRadius: 1.45,
        selectionRingScale: 2.5,
        auraColor: [0.42, 0.12, 0.08, 0.08]
      },
      combatPresentation: {
        profileId: "wild-danger",
        hitFlashColor: [0.92, 0.36, 0.24, 0.2],
        criticalRingColor: [0.92, 0.36, 0.24, 0.24],
        selectionRingColor: [0.62, 0.78, 0.92, 0.14],
        emissiveBoost: [0.04, 0.02, 0.01],
        impactScale: 1.1
      },
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
      spawnProfile: spawnProfile(
        `${slug(label)}-resource`,
        "verdant-hollow",
        skill === "Woodcutting" ? "forest-resources" : "mineral-resources",
        720,
        12
      ),
      actorPresentation: {
        profileId: skill === "Woodcutting" ? "tree-resource" : "ore-resource",
        meshAssetId: skill === "Woodcutting" ? "canopy-tree" : "weathered-boulder",
        materialPaletteId: skill === "Woodcutting" ? "verdant-resource" : "ore-seam",
        animationSetId: "static-prop",
        scaleMultiplier: 1,
        footprintRadius: 1.4,
        selectionRingScale: 2.2,
        auraColor: [0, 0, 0, 0]
      },
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
      questAnchor:
        label === "Expedition Chest"
          ? questAnchor(
              ["ember-charm-recovery"],
              "Recover the ember charm from the expedition chest",
              ["loot", "artifact"]
            )
          : null,
      actorPresentation: {
        profileId: "loot-cache",
        meshAssetId: "supply-crate",
        materialPaletteId: "bronze-cache",
        animationSetId: "static-prop",
        scaleMultiplier: 1,
        footprintRadius: 1.1,
        selectionRingScale: 2.1,
        auraColor: [0.48, 0.32, 0.08, 0.1]
      },
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
      faction: factionAffiliation("verdant-wardens", "companion", "Friendly", 10),
      actorPresentation: {
        profileId: speciesId,
        meshAssetId: "spirit-companion",
        materialPaletteId: "summon-shell",
        animationSetId: "companion-hover",
        scaleMultiplier: 1,
        footprintRadius: 0.95,
        selectionRingScale: 2.2,
        auraColor: [0.28, 0.82, 0.7, 0.18]
      },
      combatPresentation: {
        profileId: "companion-combat",
        hitFlashColor: [0.4, 0.92, 0.78, 0.18],
        criticalRingColor: [0.4, 0.92, 0.78, 0.22],
        selectionRingColor: [0.42, 0.88, 0.74, 0.28],
        emissiveBoost: [0.03, 0.08, 0.06],
        impactScale: 1.05
      },
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
      faction:
        label === "glass spire"
          ? factionAffiliation("ancient-spirekeepers", "relic", "Neutral", 30)
          : null,
      questAnchor:
        label === "glass spire"
          ? questAnchor(
              ["spire-attunement"],
              "Inspect the glass spire to attune with Verdant Hollow",
              ["exploration", "attunement"]
            )
          : null,
      actorPresentation: {
        profileId: slug(label),
        meshAssetId: null,
        materialPaletteId: "world-prop",
        animationSetId: "static-prop",
        scaleMultiplier: 1,
        footprintRadius: 1.6,
        selectionRingScale: 2.4,
        auraColor: [0, 0, 0, 0]
      },
      atmosphere:
        label === "glass spire"
          ? verdantAtmosphereProfile()
          : null,
      atmosphereVolume:
        label === "glass spire"
          ? {
              radius: 9.5,
              priority: 3
            }
          : null,
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
    chunkKey: null,
    regionId: null,
    regionName: null,
    teamId: null,
    questGraphIds: [],
    factionTrackId: null,
    encounterTableId: null,
    combatStyle: null,
    speciesId: null,
    speciesName: null,
    resourceSkill: null,
    resourceTier: null,
    encounterKind: null,
    faction: null,
    questAnchor: null,
    encounterProfile: null,
    spawnProfile: null,
    atmosphere: null,
    atmosphereVolume: null,
    actorPresentation: null,
    combatPresentation: null,
    interaction: interactionHints(),
    ...overrides
  };
}

function verdantAtmosphereProfile(): NetworkAtmosphereProfile {
  return {
    biomeId: "verdant-hollow",
    skyColor: [0.64, 0.8, 0.98, 1],
    fogColor: [0.72, 0.84, 0.78, 1],
    fogNear: 30,
    fogFar: 196,
    ambientColor: [0.82, 0.92, 0.88],
    ambientIntensity: 1.4,
    sunColor: [1, 0.96, 0.84],
    sunIntensity: 2.95,
    sunDirection: [30, 48, 18],
    fillColor: [0.48, 0.76, 0.94],
    fillIntensity: 0.88,
    fillDirection: [-18, 14, -10],
    rimColor: [0.4, 0.88, 0.78],
    rimIntensity: 8.5,
    groundColor: [0.19, 0.33, 0.21, 1],
    starfieldIntensity: 0.08
  };
}

function factionAffiliation(
  factionId: string,
  roleId: string,
  disposition: "Friendly" | "Neutral" | "Hostile",
  influenceRadius: number
) {
  return {
    factionId,
    roleId,
    disposition,
    influenceRadius
  };
}

function questAnchor(
  questIds: string[],
  primaryPrompt: string,
  stageTags: string[]
) {
  return {
    questIds,
    primaryPrompt,
    stageTags
  };
}

function encounterProfile(
  tableId: string,
  difficultyTier: number,
  recommendedPartySize: number,
  respawnTicks: number
) {
  return {
    tableId,
    difficultyTier,
    recommendedPartySize,
    respawnTicks
  };
}

function spawnProfile(
  profileId: string,
  biomeId: string,
  spawnGroup: string,
  respawnTicks: number,
  leashRadius: number
) {
  return {
    profileId,
    biomeId,
    spawnGroup,
    respawnTicks,
    leashRadius
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

function accepted(progressionDirty = false): LocalActionResult {
  return {
    rejection: null,
    progressionDirty
  };
}

function rejected(reason: string): LocalActionResult {
  return {
    rejection: reason,
    progressionDirty: false
  };
}

function cloneEntity(entity: LocalEntity): LocalEntity {
  return {
    ...entity,
    position: [...entity.position],
    velocity: [...entity.velocity],
    metadata: {
      ...entity.metadata,
      questGraphIds: entity.metadata.questGraphIds.slice(),
      faction: entity.metadata.faction ? { ...entity.metadata.faction } : null,
      questAnchor: entity.metadata.questAnchor
        ? {
            ...entity.metadata.questAnchor,
            questIds: entity.metadata.questAnchor.questIds.slice(),
            stageTags: entity.metadata.questAnchor.stageTags.slice()
          }
        : null,
      encounterProfile: entity.metadata.encounterProfile
        ? { ...entity.metadata.encounterProfile }
        : null,
      spawnProfile: entity.metadata.spawnProfile ? { ...entity.metadata.spawnProfile } : null,
      atmosphere: entity.metadata.atmosphere ? { ...entity.metadata.atmosphere } : null,
      atmosphereVolume: entity.metadata.atmosphereVolume
        ? { ...entity.metadata.atmosphereVolume }
        : null,
      actorPresentation: entity.metadata.actorPresentation
        ? { ...entity.metadata.actorPresentation }
        : null,
      combatPresentation: entity.metadata.combatPresentation
        ? { ...entity.metadata.combatPresentation }
        : null,
      interaction: { ...entity.metadata.interaction }
    },
    spawn: [...entity.spawn],
    desiredMove: entity.desiredMove ? [...entity.desiredMove] : null,
    anchor: entity.anchor ? [...entity.anchor] : null
  };
}

function currentFactionTier(track: LocalFactionTrackState): LocalFactionReputationTier {
  return track.tiers
    .slice()
    .sort((left, right) => right.minimumScore - left.minimumScore)
    .find((tier) => track.score >= tier.minimumScore) ?? track.tiers[0];
}

function activeQuestStageTags(state: LocalWorldState): Set<string> {
  const tags = new Set<string>();
  for (const graph of state.questGraphs.values()) {
    const stage = graph.stages.find((entry) => entry.stageId === graph.currentStageId);
    for (const tag of stage?.stageTags ?? []) {
      tags.add(tag);
    }
  }
  return tags;
}

function activeReputationTierIds(state: LocalWorldState): Set<string> {
  const tiers = new Set<string>();
  for (const track of state.factionTracks.values()) {
    tiers.add(currentFactionTier(track).tierId);
  }
  return tiers;
}

function emptyPopulationBreakdown(): NetworkPopulationBreakdown {
  return {
    players: 0,
    npcs: 0,
    wildCreatures: 0,
    companions: 0,
    resourceNodes: 0,
    lootContainers: 0,
    scenery: 0
  };
}

function incrementPopulationBreakdown(
  counts: NetworkPopulationBreakdown,
  entity: NetworkEntitySnapshot
): void {
  switch (entity.metadata.kind) {
    case "Player":
      counts.players += 1;
      break;
    case "Npc":
      counts.npcs += 1;
      break;
    case "WildCreature":
      counts.wildCreatures += 1;
      break;
    case "Companion":
      counts.companions += 1;
      break;
    case "ResourceNode":
      counts.resourceNodes += 1;
      break;
    case "LootContainer":
      counts.lootContainers += 1;
      break;
    default:
      counts.scenery += 1;
      break;
  }
}

function totalPopulationBreakdown(counts: NetworkPopulationBreakdown): number {
  return (
    counts.players +
    counts.npcs +
    counts.wildCreatures +
    counts.companions +
    counts.resourceNodes +
    counts.lootContainers +
    counts.scenery
  );
}

function activeSpawnedActors(counts: NetworkPopulationBreakdown): number {
  return counts.players + counts.npcs + counts.wildCreatures + counts.companions;
}

function finalizeChunkPopulation(state: NetworkChunkPopulationState): void {
  state.activeEntityCount = totalPopulationBreakdown(state.counts);
  const activeActors = activeSpawnedActors(state.counts);
  state.spawnBudgetRemaining = Math.max(0, state.ambientPopulationCap - activeActors);
  state.populationPressure =
    state.ambientPopulationCap <= 0 ? 0 : activeActors / state.ambientPopulationCap;
}

function finalizeRegionPopulation(state: NetworkRegionPopulationState): void {
  state.activeEntityCount = totalPopulationBreakdown(state.counts);
  const activeActors = activeSpawnedActors(state.counts);
  state.spawnBudgetRemaining = Math.max(0, state.ambientPopulationCap - activeActors);
  state.populationPressure =
    state.ambientPopulationCap <= 0 ? 0 : activeActors / state.ambientPopulationCap;
}

function summarizePopulationState(
  tick: number,
  entities: NetworkEntitySnapshot[],
  regions: Map<string, LocalRegionState>,
  encounterTables: Map<string, LocalEncounterTableState>
): NetworkWorldPopulationState {
  const chunks = new Map<string, NetworkChunkPopulationState>();

  for (const region of regions.values()) {
    for (const chunkKey of region.chunkKeys) {
      const ambientPopulationCap = region.encounterTableIds.reduce(
        (total, tableId) => total + (encounterTables.get(tableId)?.ambientCap ?? 0),
        0
      );
      chunks.set(chunkKey, {
        chunkKey,
        regionId: region.regionId,
        regionName: region.displayName,
        biomeId: region.primaryBiomeId,
        questGraphIds: [...region.activeQuestGraphIds],
        factionTrackId: region.dominantFactionTrackId || null,
        encounterTableIds: [...region.encounterTableIds],
        counts: emptyPopulationBreakdown(),
        activeEntityCount: 0,
        ambientPopulationCap,
        spawnBudgetRemaining: ambientPopulationCap,
        pendingRespawns: 0,
        nextRespawnTick: null,
        populationPressure: 0
      });
    }
  }

  for (const entity of entities) {
    const chunkKey = entity.metadata.chunkKey ?? chunkKeyFromPosition(entity.position);
    const region = entity.metadata.regionId
      ? regions.get(entity.metadata.regionId)
      : regionForChunkKey(chunkKey, regions);
    const chunkState =
      chunks.get(chunkKey) ??
      {
        chunkKey,
        regionId: entity.metadata.regionId ?? region?.regionId ?? null,
        regionName: entity.metadata.regionName ?? region?.displayName ?? null,
        biomeId: region?.primaryBiomeId ?? entity.metadata.spawnProfile?.biomeId ?? null,
        questGraphIds: entity.metadata.questGraphIds.slice(),
        factionTrackId:
          entity.metadata.factionTrackId ?? region?.dominantFactionTrackId ?? null,
        encounterTableIds: entity.metadata.encounterTableId
          ? [entity.metadata.encounterTableId]
          : [...(region?.encounterTableIds ?? [])],
        counts: emptyPopulationBreakdown(),
        activeEntityCount: 0,
        ambientPopulationCap: 0,
        spawnBudgetRemaining: 0,
        pendingRespawns: 0,
        nextRespawnTick: null,
        populationPressure: 0
      };

    if (chunkState.ambientPopulationCap === 0) {
      chunkState.ambientPopulationCap = chunkState.encounterTableIds.reduce(
        (total, tableId) => total + (encounterTables.get(tableId)?.ambientCap ?? 0),
        0
      );
    }
    incrementPopulationBreakdown(chunkState.counts, entity);
    finalizeChunkPopulation(chunkState);
    chunks.set(chunkKey, chunkState);
  }

  const chunkList = Array.from(chunks.values()).sort((left, right) =>
    left.chunkKey.localeCompare(right.chunkKey)
  );
  const regionList: NetworkRegionPopulationState[] = Array.from(regions.values())
    .map((region) => {
      const counts = emptyPopulationBreakdown();
      const matchingChunks = chunkList.filter(
        (chunk) => chunk.regionId === region.regionId
      );
      for (const chunk of matchingChunks) {
        counts.players += chunk.counts.players;
        counts.npcs += chunk.counts.npcs;
        counts.wildCreatures += chunk.counts.wildCreatures;
        counts.companions += chunk.counts.companions;
        counts.resourceNodes += chunk.counts.resourceNodes;
        counts.lootContainers += chunk.counts.lootContainers;
        counts.scenery += chunk.counts.scenery;
      }
      const ambientPopulationCap = region.encounterTableIds.reduce(
        (total, tableId) => total + (encounterTables.get(tableId)?.ambientCap ?? 0),
        0
      );
      const regionState: NetworkRegionPopulationState = {
        regionId: region.regionId,
        regionName: region.displayName,
        primaryBiomeId: region.primaryBiomeId,
        chunkKeys: [...region.chunkKeys],
        activeQuestGraphIds: [...region.activeQuestGraphIds],
        dominantFactionTrackId: region.dominantFactionTrackId || null,
        encounterTableIds: [...region.encounterTableIds],
        activeChunkCount: matchingChunks.filter((chunk) => chunk.activeEntityCount > 0).length,
        counts,
        activeEntityCount: 0,
        ambientPopulationCap,
        spawnBudgetRemaining: ambientPopulationCap,
        pendingRespawns: 0,
        nextRespawnTick: null,
        populationPressure: 0
      };
      finalizeRegionPopulation(regionState);
      return regionState;
    })
    .sort((left, right) => left.regionId.localeCompare(right.regionId));

  return {
    tick,
    chunks: chunkList,
    regions: regionList
  };
}

function chunkKeyFromPosition(position: Vec2Tuple): string {
  return `${Math.floor(position[0] / LOCAL_WORLD_CHUNK_SIZE)}:${Math.floor(
    position[1] / LOCAL_WORLD_CHUNK_SIZE
  )}`;
}

function expandDesiredChunkKeys(centerChunkKey: string, radius: number): string[] {
  const [centerX, centerY] = parseChunkKey(centerChunkKey);
  const keys = new Set<string>();
  for (let offsetY = -radius; offsetY <= radius; offsetY += 1) {
    for (let offsetX = -radius; offsetX <= radius; offsetX += 1) {
      keys.add(`${centerX + offsetX}:${centerY + offsetY}`);
    }
  }
  return Array.from(keys).sort((left, right) => left.localeCompare(right));
}

function parseChunkKey(chunkKey: string): [number, number] {
  const [rawX = "0", rawY = "0"] = chunkKey.split(":");
  return [Number.parseInt(rawX, 10) || 0, Number.parseInt(rawY, 10) || 0];
}

function regionForChunkKey(
  chunkKey: string,
  regions: Map<string, LocalRegionState>
): LocalRegionState | null {
  for (const region of regions.values()) {
    if (region.chunkKeys.includes(chunkKey)) {
      return region;
    }
  }
  return null;
}

function entityArchetypeId(entity: LocalEntity): string {
  if (entity.metadata.speciesId) {
    return entity.metadata.speciesId;
  }
  if (entity.metadata.spawnProfile) {
    return entity.metadata.spawnProfile.profileId;
  }
  if (entity.role === "resource") {
    return `${slug(entity.label)}-resource`;
  }
  return slug(entity.label);
}

function syncStreamingMetadataForEntity(entity: LocalEntity, state: LocalWorldState): void {
  const chunkKey = chunkKeyFromPosition(entity.position);
  const region = regionForChunkKey(chunkKey, state.regions);
  const encounterTableId =
    entity.metadata.encounterProfile?.tableId ??
    findEncounterTableIdForEntity(entity, region, state.encounterTables);

  entity.metadata.chunkKey = chunkKey;
  entity.metadata.regionId = region?.regionId ?? null;
  entity.metadata.regionName = region?.displayName ?? null;
  entity.metadata.factionTrackId =
    entity.metadata.faction?.factionId ?? region?.dominantFactionTrackId ?? null;
  entity.metadata.questGraphIds = Array.from(
    new Set([
      ...(entity.metadata.questAnchor?.questIds ?? []),
      ...(region?.activeQuestGraphIds ?? [])
    ])
  );
  entity.metadata.encounterTableId = encounterTableId;
}

function findEncounterTableIdForEntity(
  entity: LocalEntity,
  region: LocalRegionState | null,
  encounterTables: Map<string, LocalEncounterTableState>
): string | null {
  for (const tableId of region?.encounterTableIds ?? []) {
    const table = encounterTables.get(tableId);
    if (!table) {
      continue;
    }
    if (table.entries.some((entry) => entry.archetypeId === entityArchetypeId(entity))) {
      return tableId;
    }
  }
  return null;
}

function shouldTemplateEntityBeActive(entity: LocalEntity, state: LocalWorldState): boolean {
  const tableId = entity.metadata.encounterTableId;
  if (!tableId) {
    return true;
  }

  const table = state.encounterTables.get(tableId);
  if (!table) {
    return true;
  }

  const entry = table.entries.find((candidate) => candidate.archetypeId === entityArchetypeId(entity));
  if (!entry) {
    return true;
  }

  const activeTags = activeQuestStageTags(state);
  const activeTiers = activeReputationTierIds(state);
  return (
    entry.requiredStageTags.every((tag) => activeTags.has(tag)) &&
    entry.requiredReputationTiers.every((tierId) => activeTiers.has(tierId))
  );
}

function canRespawnTemplate(
  entity: LocalEntity,
  removedAtTick: number,
  currentTick: number
): boolean {
  const respawnTicks =
    entity.metadata.spawnProfile?.respawnTicks ?? entity.metadata.encounterProfile?.respawnTicks ?? null;
  return respawnTicks != null && currentTick >= removedAtTick + respawnTicks;
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

function moveScalarToward(current: number, target: number, maxDelta: number): number {
  if (current < target) {
    return Math.min(current + maxDelta, target);
  }
  return Math.max(current - maxDelta, target);
}

function rotateTowardAngle(current: number, target: number, factor: number): number {
  let delta = target - current;
  while (delta > Math.PI) {
    delta -= Math.PI * 2;
  }
  while (delta < -Math.PI) {
    delta += Math.PI * 2;
  }
  return current + delta * factor;
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
