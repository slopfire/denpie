export const GROUNDING_SOURCE_MODES = [
    "model",
    "web",
    "documents",
    "web_and_documents",
] as const;

export type GroundingSourceMode = (typeof GROUNDING_SOURCE_MODES)[number];

export const GROUNDING_BEHAVIORS = ["standard", "fact_check", "agentic"] as const;

export type GroundingBehavior = (typeof GROUNDING_BEHAVIORS)[number];

export const GROUNDING_SEARCH_DEPTHS = ["quick", "balanced", "deep"] as const;

export type GroundingSearchDepth = (typeof GROUNDING_SEARCH_DEPTHS)[number];

export const GROUNDING_FRESHNESS_OPTIONS = [
    "any",
    "week",
    "month",
    "year",
] as const;

export type GroundingFreshness = (typeof GROUNDING_FRESHNESS_OPTIONS)[number];

export const GROUNDING_FAILURE_POLICIES = ["stop", "mark_unverified"] as const;

export type GroundingFailurePolicy =
    (typeof GROUNDING_FAILURE_POLICIES)[number];

export interface GroundingPolicyDraft {
    sourceMode: GroundingSourceMode | "";
    behavior: GroundingBehavior | "";
    searchDepth: GroundingSearchDepth | "";
    freshness: GroundingFreshness | "";
    searchGuidance: string;
    includeDomains: string;
    failurePolicy: GroundingFailurePolicy | "";
}

export interface ResolvedGroundingPolicy {
    sourceMode: GroundingSourceMode;
    behavior: GroundingBehavior;
    searchDepth: GroundingSearchDepth;
    freshness: GroundingFreshness;
    searchGuidance: string;
    includeDomains: string[];
    failurePolicy: GroundingFailurePolicy;
}

export function hasGroundingSearchOverride(
    draft: GroundingPolicyDraft,
): boolean {
    return (
        draft.searchDepth !== "" ||
        draft.freshness !== "" ||
        draft.searchGuidance.trim() !== "" ||
        draft.includeDomains.trim() !== "" ||
        draft.failurePolicy !== ""
    );
}

export function clearGroundingSearchOverride(
    draft: GroundingPolicyDraft,
): GroundingPolicyDraft {
    return {
        ...draft,
        searchDepth: "",
        freshness: "",
        searchGuidance: "",
        includeDomains: "",
        failurePolicy: "",
    };
}

export function customizeGroundingSearch(
    draft: GroundingPolicyDraft,
    effective: ResolvedGroundingPolicy,
): GroundingPolicyDraft {
    return {
        ...draft,
        searchDepth: effective.searchDepth,
        freshness: effective.freshness,
        searchGuidance: effective.searchGuidance,
        includeDomains: formatDomainList(effective.includeDomains),
        failurePolicy: effective.failurePolicy,
    };
}

const DEFAULT_POLICY: ResolvedGroundingPolicy = {
    sourceMode: "model",
    behavior: "standard",
    searchDepth: "balanced",
    freshness: "any",
    searchGuidance: "",
    includeDomains: [],
    failurePolicy: "stop",
};

export function legacyGroundingPolicy(
    strategy: string,
): Pick<ResolvedGroundingPolicy, "sourceMode" | "behavior"> {
    switch (strategy) {
        case "agentic":
            return { sourceMode: "web", behavior: "agentic" };
        case "create_and_ground":
            return { sourceMode: "web", behavior: "fact_check" };
        case "rag":
            return { sourceMode: "documents", behavior: "standard" };
        case "factual":
        default:
            return { sourceMode: "model", behavior: "standard" };
    }
}

export function sourceUsesWeb(sourceMode: GroundingSourceMode): boolean {
    return sourceMode === "web" || sourceMode === "web_and_documents";
}

export function sourceUsesDocuments(sourceMode: GroundingSourceMode): boolean {
    return sourceMode === "documents" || sourceMode === "web_and_documents";
}

export function normalizeBehavior(
    sourceMode: GroundingSourceMode,
    behavior: GroundingBehavior,
): GroundingBehavior {
    return sourceMode === "model" ? "standard" : behavior;
}

export function behaviorFromLegacyVerification(value: string): GroundingBehavior {
    return value === "thorough" ? "fact_check" : "standard";
}

export function legacyVerificationForBehavior(behavior: GroundingBehavior): string {
    return behavior === "fact_check" ? "thorough" : "standard";
}

export function parseDomainList(value: string): string[] {
    const seen = new Set<string>();
    return value
        .split(/[\n,]/)
        .map((domain) => domain.trim().toLowerCase())
        .filter((domain) => {
            if (domain === "" || seen.has(domain)) return false;
            seen.add(domain);
            return true;
        });
}

export function formatDomainList(domains: readonly string[]): string {
    return domains.join("\n");
}

export function resolveGroundingPolicy(
    draft: GroundingPolicyDraft,
    inherited: ResolvedGroundingPolicy = DEFAULT_POLICY,
): ResolvedGroundingPolicy {
    const sourceMode = draft.sourceMode || inherited.sourceMode;
    const behavior = normalizeBehavior(
        sourceMode,
        draft.behavior || inherited.behavior,
    );
    return {
        sourceMode,
        behavior,
        searchDepth: draft.searchDepth || inherited.searchDepth,
        freshness: draft.freshness || inherited.freshness,
        searchGuidance: draft.searchGuidance.trim() || inherited.searchGuidance,
        includeDomains:
            draft.includeDomains.trim() === ""
                ? inherited.includeDomains
                : parseDomainList(draft.includeDomains),
        failurePolicy: draft.failurePolicy || inherited.failurePolicy,
    };
}

export function legacyStrategyForPolicy(
    policy: Pick<ResolvedGroundingPolicy, "sourceMode" | "behavior">,
): string {
    switch (policy.sourceMode) {
        case "model":
            return "factual";
        case "documents":
            return policy.behavior === "fact_check" ? "create_and_ground" : "rag";
        case "web":
            return policy.behavior === "fact_check"
                ? "create_and_ground"
                : "agentic";
        case "web_and_documents":
            return policy.behavior === "fact_check"
                ? "create_and_ground"
                : "agentic";
    }
}
