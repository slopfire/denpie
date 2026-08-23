// Dashboard session JSON for the grounding icon picker. There is no /api/v1
// op for suggest/set; the UI posts `/app/topics/suggest-icons` and
// `/app/topics/set-icon`.

export interface IconPickerTopic {
    readonly id: bigint;
    readonly name: string;
    readonly iconId: string;
    readonly topicColor: string;
}

export interface IconPickerRequest {
    readonly topicId: bigint;
    readonly generation: number;
}

export type IconPickerState =
    | { kind: "closed"; generation: number }
    | {
          kind: "suggesting";
          topic: IconPickerTopic;
          request: IconPickerRequest;
          excludedIcons: readonly string[];
      }
    | {
          kind: "ready";
          topic: IconPickerTopic;
          request: IconPickerRequest;
          suggestions: readonly string[];
          error?: string;
      }
    | {
          kind: "empty";
          topic: IconPickerTopic;
          request: IconPickerRequest;
      }
    | {
          kind: "suggestError";
          topic: IconPickerTopic;
          request: IconPickerRequest;
          message: string;
      }
    | {
          kind: "picking";
          topic: IconPickerTopic;
          request: IconPickerRequest;
          suggestions: readonly string[];
          iconId: string;
      };

export const INITIAL_ICON_PICKER_STATE: IconPickerState = {
    kind: "closed",
    generation: 0,
};

export const SUGGEST_TOPIC_ICONS_PATH = "/app/topics/suggest-icons";
export const SET_TOPIC_ICON_PATH = "/app/topics/set-icon";

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function generationOf(state: IconPickerState): number {
    return state.kind === "closed"
        ? state.generation
        : state.request.generation;
}

function sameRequest(
    left: IconPickerRequest,
    right: IconPickerRequest,
): boolean {
    return (
        left.topicId === right.topicId && left.generation === right.generation
    );
}

/** Last path segment of an Iconify id, with hyphens turned into spaces. */
export function iconShortName(icon: string): string {
    const last = icon.split(":").pop() ?? icon;
    return last.replaceAll("-", " ");
}

/** JSON number for the dashboard handlers; topic ids stay small sequential ints. */
export function jsonTopicId(id: bigint): number {
    if (id <= 0n) {
        throw new TypeError(`topic id must be positive, got ${id.toString()}`);
    }
    if (id > BigInt(Number.MAX_SAFE_INTEGER)) {
        throw new TypeError("topic id exceeds JSON integer range");
    }
    return Number(id);
}

export function parseSuggestedIcons(value: unknown): string[] {
    if (!isRecord(value) || !Array.isArray(value.icons)) {
        throw new TypeError("Suggest icons returned invalid JSON");
    }
    return value.icons.map((icon) => {
        if (typeof icon !== "string" || icon.trim() === "") {
            throw new TypeError("Suggest icons returned an invalid icon id");
        }
        return icon;
    });
}

export function parseSetTopicIcon(value: unknown): string {
    if (
        !isRecord(value) ||
        typeof value.icon_id !== "string" ||
        value.icon_id.trim() === ""
    ) {
        throw new TypeError("Set topic icon returned invalid JSON");
    }
    return value.icon_id;
}

export function openIconPicker(
    state: IconPickerState,
    topic: IconPickerTopic,
): IconPickerState {
    const generation = generationOf(state) + 1;
    return {
        kind: "suggesting",
        topic,
        request: { topicId: topic.id, generation },
        excludedIcons: [],
    };
}

export function closeIconPicker(state: IconPickerState): IconPickerState {
    return state.kind === "closed"
        ? state
        : { kind: "closed", generation: generationOf(state) + 1 };
}

export function rerollIconPicker(state: IconPickerState): IconPickerState {
    if (
        state.kind !== "ready" &&
        state.kind !== "empty" &&
        state.kind !== "suggestError"
    ) {
        return state;
    }
    const generation = generationOf(state) + 1;
    return {
        kind: "suggesting",
        topic: state.topic,
        request: { topicId: state.topic.id, generation },
        excludedIcons: state.kind === "ready" ? state.suggestions : [],
    };
}

export function suggestionsReceived(
    state: IconPickerState,
    request: IconPickerRequest,
    icons: readonly string[],
): IconPickerState {
    if (state.kind !== "suggesting" || !sameRequest(state.request, request)) {
        return state;
    }
    if (icons.length === 0) {
        return { kind: "empty", topic: state.topic, request };
    }
    return {
        kind: "ready",
        topic: state.topic,
        request,
        suggestions: icons,
    };
}

export function suggestionsFailed(
    state: IconPickerState,
    request: IconPickerRequest,
    message: string,
): IconPickerState {
    if (state.kind !== "suggesting" || !sameRequest(state.request, request)) {
        return state;
    }
    return {
        kind: "suggestError",
        topic: state.topic,
        request,
        message,
    };
}

export function startPickingIcon(
    state: IconPickerState,
    iconId: string,
): IconPickerState {
    if (state.kind !== "ready" || iconId.trim() === "") return state;
    return {
        kind: "picking",
        topic: state.topic,
        request: state.request,
        suggestions: state.suggestions,
        iconId,
    };
}

export function pickSucceeded(
    state: IconPickerState,
    request: IconPickerRequest,
): IconPickerState {
    if (state.kind !== "picking" || !sameRequest(state.request, request)) {
        return state;
    }
    return { kind: "closed", generation: request.generation + 1 };
}

export function pickFailed(
    state: IconPickerState,
    request: IconPickerRequest,
    message: string,
): IconPickerState {
    if (state.kind !== "picking" || !sameRequest(state.request, request)) {
        return state;
    }
    return {
        kind: "ready",
        topic: state.topic,
        request: state.request,
        suggestions: state.suggestions,
        error: message,
    };
}

export function applyTopicIcon<T extends { id: bigint; iconId: string }>(
    topics: readonly T[],
    topicId: bigint,
    iconId: string,
): T[] {
    return topics.map((topic) =>
        topic.id === topicId ? { ...topic, iconId } : topic,
    );
}

export function pickerTopicFrom(topic: IconPickerTopic): IconPickerTopic {
    return {
        id: topic.id,
        name: topic.name,
        iconId: topic.iconId,
        topicColor: topic.topicColor,
    };
}

async function readErrorMessage(response: Response): Promise<string> {
    const text = (await response.text()).trim();
    return text === "" ? `Request failed with status ${response.status}` : text;
}

export async function suggestTopicIcons({
    id,
    excludedIcons = [],
    fetchImpl = fetch,
}: {
    id: bigint;
    excludedIcons?: readonly string[];
    fetchImpl?: typeof fetch;
}): Promise<string[]> {
    const response = await fetchImpl(SUGGEST_TOPIC_ICONS_PATH, {
        method: "POST",
        credentials: "same-origin",
        headers: {
            accept: "application/json",
            "content-type": "application/json",
        },
        body: JSON.stringify({
            id: jsonTopicId(id),
            excluded_icons: [...excludedIcons],
        }),
    });
    if (!response.ok) {
        throw new Error(await readErrorMessage(response));
    }
    return parseSuggestedIcons(await response.json());
}

export async function setTopicIcon({
    id,
    iconId,
    fetchImpl = fetch,
}: {
    id: bigint;
    iconId: string;
    fetchImpl?: typeof fetch;
}): Promise<string> {
    const response = await fetchImpl(SET_TOPIC_ICON_PATH, {
        method: "POST",
        credentials: "same-origin",
        headers: {
            accept: "application/json",
            "content-type": "application/json",
        },
        body: JSON.stringify({
            id: jsonTopicId(id),
            icon_id: iconId,
        }),
    });
    if (!response.ok) {
        throw new Error(await readErrorMessage(response));
    }
    return parseSetTopicIcon(await response.json());
}
