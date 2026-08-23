import { create } from "@bufbuild/protobuf";
import { UpdateSettingsRequestSchema } from "@/generated/denpie_pb";
import type { Settings, UpdateSettingsRequest } from "@/generated/denpie_pb";

/** Browser-editable representation of the settings fields exposed by this page. */
export interface SettingsDraft {
    model: string;
    compressModel: string;
    template: string;
    apiKey: string;
    baseUrl: string;
    compressBaseUrl: string;
    reasoningEffort: string;
    compressReasoningEffort: string;
    compressionLevel: string;
    groundingModel: string;
    groundingReasoningEffort: string;
    groundingStrategy: string;
    imageStrategy: string;
    searchProvider: string;
    scrapeProvider: string;
    searchApiKey: string;
    searchBaseUrl: string;
    visionModel: string;
    dailyTimeZone: string;
    dailyUpdateTime: string;
    maxActiveCards: string;
    colorScheme: string;
    transparency: string;
    blurIntensity: string;
    autoupdateEnabled: boolean;
    autoupdateRepo: string;
    autoupdateBranch: string;
    autoupdateCheckIntervalSecs: string;
    autoupdateCommand: string;
}

export function settingsDraft(settings: Settings): SettingsDraft {
    return {
        model: settings.model,
        compressModel: settings.compressModel,
        template: settings.template,
        apiKey: settings.apiKey,
        baseUrl: settings.baseUrl,
        compressBaseUrl: settings.compressBaseUrl,
        reasoningEffort: settings.reasoningEffort,
        compressReasoningEffort: settings.compressReasoningEffort,
        compressionLevel: settings.compressionLevel,
        groundingModel: settings.groundingModel,
        groundingReasoningEffort: settings.groundingReasoningEffort,
        groundingStrategy: settings.groundingStrategy,
        imageStrategy: settings.imageStrategy,
        searchProvider: settings.searchProvider,
        scrapeProvider: settings.scrapeProvider,
        searchApiKey: settings.searchApiKey,
        searchBaseUrl: settings.searchBaseUrl,
        visionModel: settings.visionModel,
        dailyTimeZone: settings.dailyTimeZone,
        dailyUpdateTime: settings.dailyUpdateTime,
        maxActiveCards: settings.maxActiveCards.toString(),
        colorScheme: settings.colorScheme,
        transparency: settings.transparency,
        blurIntensity: settings.blurIntensity,
        autoupdateEnabled: settings.autoupdateEnabled,
        autoupdateRepo: settings.autoupdateRepo,
        autoupdateBranch: settings.autoupdateBranch,
        autoupdateCheckIntervalSecs:
            settings.autoupdateCheckIntervalSecs.toString(),
        autoupdateCommand: settings.autoupdateCommand,
    };
}

/** Empty/invalid numeric fields become zero, matching the server's unlimited default. */
export function parseUnsignedSetting(value: string): bigint {
    const normalized = value.trim();
    if (!/^\d+$/.test(normalized)) return 0n;
    return BigInt(normalized);
}

/** Builds a minimal update payload: unchanged settings never cross the wire. */
export function settingsPatch(
    previous: SettingsDraft,
    current: SettingsDraft,
): UpdateSettingsRequest {
    const patch = create(UpdateSettingsRequestSchema, {});
    if (current.model !== previous.model) patch.model = current.model;
    if (current.compressModel !== previous.compressModel)
        patch.compressModel = current.compressModel;
    if (current.template !== previous.template)
        patch.template = current.template;
    if (current.apiKey !== previous.apiKey) patch.apiKey = current.apiKey;
    if (current.baseUrl !== previous.baseUrl) patch.baseUrl = current.baseUrl;
    if (current.compressBaseUrl !== previous.compressBaseUrl)
        patch.compressBaseUrl = current.compressBaseUrl;
    if (current.reasoningEffort !== previous.reasoningEffort)
        patch.reasoningEffort = current.reasoningEffort;
    if (current.compressReasoningEffort !== previous.compressReasoningEffort)
        patch.compressReasoningEffort = current.compressReasoningEffort;
    if (current.compressionLevel !== previous.compressionLevel)
        patch.compressionLevel = current.compressionLevel;
    if (current.groundingModel !== previous.groundingModel)
        patch.groundingModel = current.groundingModel;
    if (current.groundingReasoningEffort !== previous.groundingReasoningEffort)
        patch.groundingReasoningEffort = current.groundingReasoningEffort;
    if (current.groundingStrategy !== previous.groundingStrategy)
        patch.groundingStrategy = current.groundingStrategy;
    if (current.imageStrategy !== previous.imageStrategy)
        patch.imageStrategy = current.imageStrategy;
    if (current.searchProvider !== previous.searchProvider)
        patch.searchProvider = current.searchProvider;
    if (current.scrapeProvider !== previous.scrapeProvider)
        patch.scrapeProvider = current.scrapeProvider;
    if (current.searchApiKey !== previous.searchApiKey)
        patch.searchApiKey = current.searchApiKey;
    if (current.searchBaseUrl !== previous.searchBaseUrl)
        patch.searchBaseUrl = current.searchBaseUrl;
    if (current.visionModel !== previous.visionModel)
        patch.visionModel = current.visionModel;
    if (current.dailyTimeZone !== previous.dailyTimeZone)
        patch.dailyTimeZone = current.dailyTimeZone;
    if (current.dailyUpdateTime !== previous.dailyUpdateTime)
        patch.dailyUpdateTime = current.dailyUpdateTime;
    if (current.maxActiveCards !== previous.maxActiveCards)
        patch.maxActiveCards = parseUnsignedSetting(current.maxActiveCards);
    if (current.colorScheme !== previous.colorScheme)
        patch.colorScheme = current.colorScheme;
    if (current.transparency !== previous.transparency)
        patch.transparency = current.transparency;
    if (current.blurIntensity !== previous.blurIntensity)
        patch.blurIntensity = current.blurIntensity;
    if (current.autoupdateEnabled !== previous.autoupdateEnabled)
        patch.autoupdateEnabled = current.autoupdateEnabled;
    if (current.autoupdateRepo !== previous.autoupdateRepo)
        patch.autoupdateRepo = current.autoupdateRepo;
    if (current.autoupdateBranch !== previous.autoupdateBranch)
        patch.autoupdateBranch = current.autoupdateBranch;
    if (
        current.autoupdateCheckIntervalSecs !==
        previous.autoupdateCheckIntervalSecs
    ) {
        patch.autoupdateCheckIntervalSecs = parseUnsignedSetting(
            current.autoupdateCheckIntervalSecs,
        );
    }
    if (current.autoupdateCommand !== previous.autoupdateCommand)
        patch.autoupdateCommand = current.autoupdateCommand;
    return patch;
}

export function hasSettingsPatch(patch: UpdateSettingsRequest): boolean {
    return settingsPatchCount(patch) > 0;
}

export function settingsPatchCount(patch: UpdateSettingsRequest): number {
    return SETTINGS_PATCH_KEYS.filter((key) => patch[key] !== undefined).length;
}

const SETTINGS_PATCH_KEYS = [
    "model",
    "compressModel",
    "template",
    "apiKey",
    "baseUrl",
    "compressBaseUrl",
    "reasoningEffort",
    "compressReasoningEffort",
    "compressionLevel",
    "groundingModel",
    "groundingReasoningEffort",
    "groundingStrategy",
    "imageStrategy",
    "searchProvider",
    "scrapeProvider",
    "searchApiKey",
    "searchBaseUrl",
    "visionModel",
    "dailyTimeZone",
    "dailyUpdateTime",
    "maxActiveCards",
    "colorScheme",
    "transparency",
    "blurIntensity",
    "autoupdateEnabled",
    "autoupdateRepo",
    "autoupdateBranch",
    "autoupdateCheckIntervalSecs",
    "autoupdateCommand",
] as const satisfies readonly (keyof UpdateSettingsRequest)[];
