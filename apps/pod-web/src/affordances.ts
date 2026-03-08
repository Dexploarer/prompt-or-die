import type { NetworkEntitySnapshot } from "./contracts";

export function formatTargetSummary(
  target: NetworkEntitySnapshot | null,
  controlled: NetworkEntitySnapshot | null
): string {
  if (!target) {
    return "No target selected";
  }

  const parts = [`${displayName(target)} · E(${target.id})`];
  const descriptor = describeTargetKind(target);
  if (descriptor) {
    parts.push(descriptor);
  }
  const faction = factionSummary(target);
  if (faction) {
    parts.push(faction);
  }
  const questHooks = questHookSummary(target);
  if (questHooks) {
    parts.push(questHooks);
  }
  if (target.metadata.teamId != null) {
    parts.push(`team ${target.metadata.teamId}`);
  }
  const health = healthSummary(target);
  if (health) {
    parts.push(health);
  }

  if (controlled) {
    const distance = Math.hypot(
      target.position[0] - controlled.position[0],
      target.position[1] - controlled.position[1]
    );
    parts.push(`${distance.toFixed(0)}u away`);
  }

  return parts.join(" · ");
}

export function describeTargetAffordances(
  target: NetworkEntitySnapshot | null
): string {
  if (!target) {
    return "Tab to cycle targets";
  }

  const actions = authoritativeAffordances(target);
  if (actions.length > 0) {
    return actions.join(" · ");
  }

  const label = target.label?.toLowerCase() ?? "";
  if (label.includes("wall") || label.includes("obstacle")) {
    return "Static scenery";
  }
  if (
    label.includes("resource") ||
    label.includes("ore") ||
    label.includes("tree") ||
    label.includes("node")
  ) {
    return "G gather · E inspect";
  }
  if (
    label.includes("loot") ||
    label.includes("chest") ||
    label.includes("cache") ||
    label.includes("corpse")
  ) {
    return "R loot · E inspect";
  }
  if (
    label.includes("monster") ||
    label.includes("creature") ||
    label.includes("beast") ||
    label.includes("wild")
  ) {
    return "Space attack · C capture · E inspect";
  }
  if (
    label.includes("companion") ||
    label.includes("pet") ||
    label.includes("summon") ||
    label.includes("spirit")
  ) {
    return "F command companion · E inspect";
  }
  if (label.includes("player") || label.includes("npc")) {
    return "Space attack · E interact · Enter chat";
  }

  return "E interact";
}

function healthSummary(target: NetworkEntitySnapshot): string | null {
  if (
    target.health == null ||
    target.maxHealth == null ||
    target.maxHealth <= 0
  ) {
    return null;
  }

  return `${target.health.toFixed(0)}/${target.maxHealth.toFixed(0)} hp`;
}

function displayName(target: NetworkEntitySnapshot): string {
  return (
    target.metadata.speciesName ??
    target.label ??
    fallbackKindLabel(target.metadata.kind) ??
    "Target"
  );
}

function describeTargetKind(target: NetworkEntitySnapshot): string | null {
  switch (target.metadata.kind) {
    case "WildCreature":
      return "wild creature";
    case "Companion":
      return "companion";
    case "ResourceNode":
      return target.metadata.resourceSkill
        ? `${target.metadata.resourceSkill.toLowerCase()} node`
        : "resource node";
    case "LootContainer":
      return "loot cache";
    case "Npc":
      return target.metadata.combatStyle
        ? `${target.metadata.combatStyle.toLowerCase()} npc`
        : "npc";
    case "Player":
      return "player";
    case "Scenery":
      return "scenery";
    default:
      return null;
  }
}

function factionSummary(target: NetworkEntitySnapshot): string | null {
  const faction = target.metadata.faction;
  if (!faction) {
    return null;
  }

  return `${faction.factionId} ${faction.roleId}`;
}

function questHookSummary(target: NetworkEntitySnapshot): string | null {
  const questAnchor = target.metadata.questAnchor;
  if (!questAnchor || questAnchor.questIds.length === 0) {
    return null;
  }

  return questAnchor.questIds.length === 1
    ? "1 quest"
    : `${questAnchor.questIds.length} quests`;
}

function authoritativeAffordances(target: NetworkEntitySnapshot): string[] {
  const actions: string[] = [];
  const hints = target.metadata.interaction;

  if (target.metadata.kind === "Scenery") {
    if ((target.metadata.questAnchor?.questIds.length ?? 0) > 0) {
      return ["Q quests", "Static scenery"];
    }
    return ["Static scenery"];
  }
  if ((target.metadata.questAnchor?.questIds.length ?? 0) > 0) {
    actions.push("Q quests");
  }
  if (hints.canAttack) {
    actions.push("Space attack");
  }
  if (hints.canCapture) {
    actions.push("C capture");
  }
  if (hints.canGather) {
    actions.push("G gather");
  }
  if (hints.canLoot) {
    actions.push("R loot");
  }
  if (hints.canCommandCompanion) {
    actions.push("F command companion");
  }
  if (hints.canInteract) {
    actions.push("E interact");
  } else if (hints.canInspect) {
    actions.push("E inspect");
  }
  if (hints.canChat) {
    actions.push("Enter chat");
  }

  return actions;
}

function fallbackKindLabel(kind: NetworkEntitySnapshot["metadata"]["kind"]): string | null {
  switch (kind) {
    case "WildCreature":
      return "Wild Creature";
    case "Companion":
      return "Companion";
    case "ResourceNode":
      return "Resource Node";
    case "LootContainer":
      return "Loot Cache";
    case "Npc":
      return "NPC";
    case "Player":
      return "Player";
    case "Scenery":
      return "Scenery";
    default:
      return null;
  }
}
