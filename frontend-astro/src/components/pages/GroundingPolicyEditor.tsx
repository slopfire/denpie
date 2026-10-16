import type { ReactNode } from "react";
import {
    Field,
    FieldDescription,
    FieldGroup,
    FieldLabel,
    FieldLegend,
    FieldSet,
} from "@/components/ui/field";
import {
    Select,
    SelectContent,
    SelectGroup,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
    clearGroundingSearchOverride,
    customizeGroundingSearch,
    hasGroundingSearchOverride,
    resolveGroundingPolicy,
    sourceUsesDocuments,
    sourceUsesWeb,
    type GroundingFailurePolicy,
    type GroundingFreshness,
    type GroundingPolicyDraft,
    type GroundingSearchDepth,
    type GroundingSourceMode,
    type GroundingBehavior,
    type ResolvedGroundingPolicy,
} from "@/lib/grounding-policy";
import { t } from "@/lib/i18n";

type SourceChoice = GroundingSourceMode | "inherit";

const CHOICE_CLASS_NAME =
    "h-auto min-h-8 min-w-0 whitespace-normal px-2 py-2 text-center";

function sourceChoice(value: string | undefined): SourceChoice | null {
    switch (value) {
        case "inherit":
        case "model":
        case "web":
        case "documents":
        case "web_and_documents":
            return value;
        default:
            return null;
    }
}

function behaviorChoice(
    value: string | undefined,
): GroundingBehavior | "inherit" | null {
    switch (value) {
        case "inherit":
        case "standard":
        case "fact_check":
        case "agentic":
            return value;
        default:
            return null;
    }
}

function sourceDescription(value: GroundingSourceMode): string {
    switch (value) {
        case "model":
            return t("grounding.policy.source_model_description");
        case "web":
            return t("grounding.policy.source_web_description");
        case "documents":
            return t("grounding.policy.source_documents_description");
        case "web_and_documents":
            return t("grounding.policy.source_web_and_documents_description");
    }
}

function behaviorDescription(value: GroundingBehavior): string {
    switch (value) {
        case "standard":
            return t("grounding.policy.behavior_standard_description");
        case "fact_check":
            return t("grounding.policy.behavior_fact_check_description");
        case "agentic":
            return t("grounding.policy.behavior_agentic_description");
    }
}

function policyValue(
    value: GroundingPolicyDraft,
    inherited?: ResolvedGroundingPolicy,
): ResolvedGroundingPolicy {
    return resolveGroundingPolicy(value, inherited);
}

export function GroundingPolicyEditor({
    value,
    inherited,
    allowInherit = false,
    disabled = false,
    onChange,
    documentControls,
    searchProvider,
    onSearchProviderChange,
}: {
    value: GroundingPolicyDraft;
    inherited?: ResolvedGroundingPolicy;
    allowInherit?: boolean;
    disabled?: boolean;
    onChange: (next: GroundingPolicyDraft) => void;
    documentControls?: ReactNode;
    searchProvider: string;
    onSearchProviderChange?: (provider: string) => void;
}) {
    const effective = policyValue(value, inherited);
    const searchIsCustomized = hasGroundingSearchOverride(value);
    const selectedSource: SourceChoice = value.sourceMode || "inherit";
    const selectedBehavior = value.behavior || "inherit";
    const update = (patch: Partial<GroundingPolicyDraft>) =>
        onChange({ ...value, ...patch });

    return (
        <FieldGroup>
            <FieldSet>
                <FieldLegend>{t("grounding.policy.source")}</FieldLegend>
                <FieldDescription>
                    {t("grounding.policy.source_description")}
                </FieldDescription>
                <ToggleGroup
                    variant="outline"
                    value={[selectedSource]}
                    disabled={disabled}
                    aria-label={t("grounding.policy.source")}
                    className="grid w-full grid-cols-1 items-stretch @sm/field-group:grid-cols-2 @2xl/field-group:grid-cols-3"
                    onValueChange={(next) => {
                        const selected = sourceChoice(next[0]);
                        if (selected === null) return;
                        update({
                            sourceMode: selected === "inherit" ? "" : selected,
                            behavior:
                                selected === "model" ? "standard" : value.behavior,
                        });
                    }}
                >
                    {allowInherit ? (
                        <ToggleGroupItem
                            value="inherit"
                            className={CHOICE_CLASS_NAME}
                        >
                            {t("grounding.policy.inherit")}
                        </ToggleGroupItem>
                    ) : null}
                    <ToggleGroupItem
                        value="model"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.source_model")}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="web" className={CHOICE_CLASS_NAME}>
                        {t("grounding.policy.source_web")}
                    </ToggleGroupItem>
                    <ToggleGroupItem
                        value="documents"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.source_documents")}
                    </ToggleGroupItem>
                    <ToggleGroupItem
                        value="web_and_documents"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.source_web_and_documents")}
                    </ToggleGroupItem>
                </ToggleGroup>
                <FieldDescription>
                    {sourceDescription(effective.sourceMode)}
                </FieldDescription>
            </FieldSet>

            {sourceUsesDocuments(effective.sourceMode) && documentControls}

            <FieldSet
                data-disabled={effective.sourceMode === "model" || undefined}
            >
                <FieldLegend>{t("grounding.policy.behavior")}</FieldLegend>
                <FieldDescription>
                    {effective.sourceMode === "model"
                        ? t("grounding.policy.behavior_model_description")
                        : t("grounding.policy.behavior_description")}
                </FieldDescription>
                <ToggleGroup
                    variant="outline"
                    value={[
                        effective.sourceMode === "model"
                            ? "standard"
                            : selectedBehavior,
                    ]}
                    disabled={disabled || effective.sourceMode === "model"}
                    aria-label={t("grounding.policy.behavior")}
                    className="grid w-full grid-cols-1 items-stretch @sm/field-group:grid-cols-2 @2xl/field-group:grid-cols-4"
                    onValueChange={(next) => {
                        const selected = behaviorChoice(next[0]);
                        if (selected === null) return;
                        update({
                            behavior: selected === "inherit" ? "" : selected,
                        });
                    }}
                >
                    {allowInherit ? (
                        <ToggleGroupItem
                            value="inherit"
                            className={CHOICE_CLASS_NAME}
                        >
                            {t("grounding.policy.inherit")}
                        </ToggleGroupItem>
                    ) : null}
                    <ToggleGroupItem
                        value="standard"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.behavior_standard")}
                    </ToggleGroupItem>
                    <ToggleGroupItem
                        value="fact_check"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.behavior_fact_check")}
                    </ToggleGroupItem>
                    <ToggleGroupItem
                        value="agentic"
                        className={CHOICE_CLASS_NAME}
                    >
                        {t("grounding.policy.behavior_agentic")}
                    </ToggleGroupItem>
                </ToggleGroup>
                {effective.sourceMode !== "model" ? (
                    <FieldDescription>
                        {behaviorDescription(effective.behavior)}
                    </FieldDescription>
                ) : null}
            </FieldSet>

            {sourceUsesWeb(effective.sourceMode) ? (
                <FieldSet data-testid="grounding-web-search">
                    <FieldLegend>
                        {t("grounding.policy.web_search")}
                    </FieldLegend>
                    <FieldDescription>
                        {t("grounding.policy.web_search_description")}
                    </FieldDescription>
                    <PolicySelect<string>
                        label={t("settings.grounding.search_provider")}
                        value={searchProvider}
                        options={[
                            ["native", t("settings.providers.native")],
                            ["tavily", t("settings.providers.tavily")],
                            ["firecrawl", t("settings.providers.firecrawl")],
                        ]}
                        disabled={disabled || onSearchProviderChange === undefined}
                        onValueChange={(provider) =>
                            onSearchProviderChange?.(provider)
                        }
                    />
                    {onSearchProviderChange === undefined ? (
                        <FieldDescription>
                            {t("grounding.policy.provider_inherited")}
                        </FieldDescription>
                    ) : null}
                    {allowInherit ? (
                        <ToggleGroup
                            variant="outline"
                            value={[searchIsCustomized ? "custom" : "inherit"]}
                            disabled={disabled}
                            aria-label={t("grounding.policy.web_search")}
                            className="grid w-full grid-cols-1 items-stretch @sm/field-group:grid-cols-2"
                            onValueChange={(next) => {
                                switch (next[0]) {
                                    case "inherit":
                                        onChange(
                                            clearGroundingSearchOverride(value),
                                        );
                                        break;
                                    case "custom":
                                        onChange(
                                            customizeGroundingSearch(
                                                value,
                                                effective,
                                            ),
                                        );
                                        break;
                                }
                            }}
                        >
                            <ToggleGroupItem
                                value="inherit"
                                className={CHOICE_CLASS_NAME}
                            >
                                {t("grounding.policy.inherit")}
                            </ToggleGroupItem>
                            <ToggleGroupItem
                                value="custom"
                                className={CHOICE_CLASS_NAME}
                            >
                                {t("grounding.policy.search_custom")}
                            </ToggleGroupItem>
                        </ToggleGroup>
                    ) : null}
                    {!allowInherit || searchIsCustomized ? (
                        <FieldGroup>
                            <div className="grid gap-3 sm:grid-cols-2">
                                <PolicySelect<GroundingSearchDepth>
                                    label={t("grounding.policy.depth")}
                                    value={
                                        value.searchDepth ||
                                        effective.searchDepth
                                    }
                                    options={[
                                        [
                                            "quick",
                                            t("grounding.policy.depth_quick"),
                                        ],
                                        [
                                            "balanced",
                                            t(
                                                "grounding.policy.depth_balanced",
                                            ),
                                        ],
                                        [
                                            "deep",
                                            t("grounding.policy.depth_deep"),
                                        ],
                                    ]}
                                    disabled={disabled}
                                    onValueChange={(searchDepth) =>
                                        update({ searchDepth })
                                    }
                                />
                                <PolicySelect<GroundingFreshness>
                                    label={t("grounding.policy.freshness")}
                                    value={
                                        value.freshness || effective.freshness
                                    }
                                    options={[
                                        [
                                            "any",
                                            t("grounding.policy.freshness_any"),
                                        ],
                                        [
                                            "week",
                                            t(
                                                "grounding.policy.freshness_week",
                                            ),
                                        ],
                                        [
                                            "month",
                                            t(
                                                "grounding.policy.freshness_month",
                                            ),
                                        ],
                                        [
                                            "year",
                                            t(
                                                "grounding.policy.freshness_year",
                                            ),
                                        ],
                                    ]}
                                    disabled={disabled}
                                    onValueChange={(freshness) =>
                                        update({ freshness })
                                    }
                                />
                            </div>
                            <Field>
                                <FieldLabel htmlFor="grounding-search-guidance">
                                    {t("grounding.policy.guidance")}
                                </FieldLabel>
                                <Textarea
                                    id="grounding-search-guidance"
                                    rows={2}
                                    value={value.searchGuidance}
                                    placeholder={t(
                                        "grounding.policy.guidance_placeholder",
                                    )}
                                    disabled={disabled}
                                    onChange={(event) =>
                                        update({
                                            searchGuidance: event.target.value,
                                        })
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel htmlFor="grounding-search-domains">
                                    {t("grounding.policy.domains")}
                                </FieldLabel>
                                <Textarea
                                    id="grounding-search-domains"
                                    rows={2}
                                    value={value.includeDomains}
                                    placeholder={t(
                                        "grounding.policy.domains_placeholder",
                                    )}
                                    disabled={disabled}
                                    onChange={(event) =>
                                        update({
                                            includeDomains: event.target.value,
                                        })
                                    }
                                />
                                <FieldDescription>
                                    {t("grounding.policy.domains_description")}
                                </FieldDescription>
                            </Field>
                            <PolicySelect<GroundingFailurePolicy>
                                label={t("grounding.policy.failure")}
                                value={
                                    value.failurePolicy ||
                                    effective.failurePolicy
                                }
                                options={[
                                    [
                                        "stop",
                                        t("grounding.policy.failure_stop"),
                                    ],
                                    [
                                        "mark_unverified",
                                        t(
                                            "grounding.policy.failure_mark_unverified",
                                        ),
                                    ],
                                ]}
                                disabled={disabled}
                                onValueChange={(failurePolicy) =>
                                    update({ failurePolicy })
                                }
                            />
                        </FieldGroup>
                    ) : null}
                </FieldSet>
            ) : null}
        </FieldGroup>
    );
}

function PolicySelect<Value extends string>({
    label,
    value,
    options,
    disabled,
    onValueChange,
}: {
    label: string;
    value: Value;
    options: readonly (readonly [Value, string])[];
    disabled: boolean;
    onValueChange: (value: Value) => void;
}) {
    return (
        <Field>
            <FieldLabel>{label}</FieldLabel>
            <Select
                value={value}
                disabled={disabled}
                onValueChange={(next) => {
                    const selected = options.find(
                        ([candidate]) => candidate === next,
                    );
                    if (selected !== undefined) onValueChange(selected[0]);
                }}
            >
                <SelectTrigger className="w-full">
                    <SelectValue />
                </SelectTrigger>
                <SelectContent>
                    <SelectGroup>
                        {options.map(([optionValue, optionLabel]) => (
                            <SelectItem key={optionValue} value={optionValue}>
                                {optionLabel}
                            </SelectItem>
                        ))}
                    </SelectGroup>
                </SelectContent>
            </Select>
        </Field>
    );
}
