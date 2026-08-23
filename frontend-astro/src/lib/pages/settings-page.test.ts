import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SettingsSchema } from "@/generated/denpie_pb";
import {
    hasSettingsPatch,
    parseUnsignedSetting,
    settingsDraft,
    settingsPatch,
    settingsPatchCount,
} from "./settings-page";

describe("settings page helpers", () => {
    test("serializes uint64 fields without losing bigint precision", () => {
        const draft = settingsDraft(
            create(SettingsSchema, {
                maxActiveCards: 9_007_199_254_740_993n,
                autoupdateCheckIntervalSecs: 120n,
            }),
        );

        expect(draft.maxActiveCards).toBe("9007199254740993");
        expect(parseUnsignedSetting(draft.maxActiveCards)).toBe(
            9_007_199_254_740_993n,
        );
        expect(parseUnsignedSetting("-1")).toBe(0n);
        expect(parseUnsignedSetting("garbage")).toBe(0n);
    });

    test("creates only the settings fields changed by the user", () => {
        const initial = settingsDraft(
            create(SettingsSchema, {
                model: "gpt-5",
                maxActiveCards: 10n,
                autoupdateEnabled: false,
            }),
        );
        const current = {
            ...initial,
            model: "gpt-5.1",
            maxActiveCards: "12",
            autoupdateEnabled: true,
        };

        const patch = settingsPatch(initial, current);

        expect(patch.model).toBe("gpt-5.1");
        expect(patch.maxActiveCards).toBe(12n);
        expect(patch.autoupdateEnabled).toBe(true);
        expect(settingsPatchCount(patch)).toBe(3);
        expect(hasSettingsPatch(patch)).toBe(true);
        expect(hasSettingsPatch(settingsPatch(initial, initial))).toBe(false);
    });
});
