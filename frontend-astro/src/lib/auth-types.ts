/**
 * Auth/session domain types and HTTP-boundary parsers.
 *
 * Pure module: no DOM, no fetch, no React — safe to import and test under Bun.
 * All external JSON is parsed/validated here at the boundary; nothing past
 * this module trusts an untyped payload.
 */

/** `/auth/me` success body, exactly as served by Axum. */
export interface SessionUser {
  id: string;
  username: string;
  role: string;
  display_name: string | null;
  avatar_data: string | null;
  build_sha: string;
}

/** Discriminated union of every auth/session state the UI can render. */
export type SessionState =
  | { status: "checking" }
  | { status: "guest" }
  | { status: "authenticated"; user: SessionUser; notice?: string }
  | { status: "error"; message: string };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown): string {
  if (typeof value !== "string") {
    throw new TypeError("expected string field");
  }
  return value;
}

function asStringOrNull(value: unknown): string | null {
  if (value === null) return null;
  return asString(value);
}

/**
 * Parse an untyped `/auth/me` JSON body into a validated {@link SessionUser}.
 * Throws on any shape mismatch — callers translate that into an error state.
 */
export function parseSessionUser(json: unknown): SessionUser {
  if (!isRecord(json)) {
    throw new TypeError("/auth/me body is not an object");
  }
  const allowed = new Set([
    "id",
    "username",
    "role",
    "display_name",
    "avatar_data",
    "build_sha",
  ]);
  for (const key of Object.keys(json)) {
    if (!allowed.has(key)) {
      throw new TypeError(`/auth/me unexpected field: ${key}`);
    }
  }
  return {
    id: asString(json.id),
    username: asString(json.username),
    role: asString(json.role),
    display_name: asStringOrNull(json.display_name),
    avatar_data: asStringOrNull(json.avatar_data),
    build_sha: asString(json.build_sha),
  };
}
