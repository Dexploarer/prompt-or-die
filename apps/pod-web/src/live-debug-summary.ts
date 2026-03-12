import type {
  AgentTickRollupDocument,
  AgentToolCallEventDocument,
  TickRollupSummary,
  ToolCallEventSummary
} from "./contracts";

export function summarizeAgentToolCallEvent(
  event: AgentToolCallEventDocument
): ToolCallEventSummary {
  return {
    agentEntityId: event.agent_entity_id,
    toolName: event.trace.tool_name,
    provider: event.trace.provider,
    status: event.trace.status,
    latencyMs: event.trace.latency_ms,
    requestUnits: event.trace.request_units,
    responseUnits: event.trace.response_units,
    errorMessage: event.trace.error_message ?? null
  };
}

export function summarizeAgentTickRollup(
  rollup: AgentTickRollupDocument
): TickRollupSummary {
  return {
    agentEntityId: rollup.agent_entity_id,
    tickStart: rollup.tick_start,
    tickEnd: rollup.tick_end,
    totalDistance: Number(rollup.total_distance.toFixed(2)),
    rejectedActionCount: rollup.rejected_action_count,
    toolErrorCount: rollup.tool_error_count,
    averageToolLatencyMs: Number(rollup.average_tool_latency_ms.toFixed(2)),
    visibleEntityCount: rollup.visible_entity_count,
    audibleEventCount: rollup.audible_event_count,
    messageCount: rollup.message_count
  };
}
