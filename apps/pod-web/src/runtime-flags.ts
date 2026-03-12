function parseBooleanQueryValue(value: string | null): boolean {
  if (value == null) {
    return false;
  }

  switch (value.trim().toLowerCase()) {
    case "1":
    case "true":
    case "yes":
    case "on":
      return true;
    default:
      return false;
  }
}

export function resolveFixedTimeMs(search: string): number | null {
  const params = new URLSearchParams(search);
  const value = params.get("fixedTimeMs");
  if (value == null) {
    return null;
  }

  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

export function shouldPauseInteractiveRuntime(search: string): boolean {
  const params = new URLSearchParams(search);
  return (
    parseBooleanQueryValue(params.get("paused")) ||
    parseBooleanQueryValue(params.get("pause"))
  );
}
