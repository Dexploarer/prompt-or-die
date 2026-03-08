import type { NetworkEntitySnapshot } from "./contracts";

export function formatTargetSummary(
  target: NetworkEntitySnapshot | null,
  controlled: NetworkEntitySnapshot | null
): string {
  if (!target) {
    return "No target selected";
  }

  const parts = [`${target.label ?? "Target"} · E(${target.id})`];
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
