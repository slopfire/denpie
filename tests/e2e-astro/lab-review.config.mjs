import { existsSync } from "node:fs";
import { defineConfig, devices } from "@playwright/test";

const chromiumExecutable = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
].find((candidate) => candidate !== undefined && existsSync(candidate));

export default defineConfig({
    testDir: ".",
    testMatch: "lab-review.spec.ts",
    workers: 1,
    reporter: "list",
    use: {
        baseURL: "http://127.0.0.1:3027",
        trace: "retain-on-failure",
        screenshot: "only-on-failure",
    },
    projects: [
        {
            name: "chromium",
            use: {
                ...devices["Desktop Chrome"],
                launchOptions:
                    chromiumExecutable === undefined
                        ? undefined
                        : { executablePath: chromiumExecutable },
            },
        },
    ],
    webServer: {
        command:
            "cd ../.. && DENPIE_LAB_REVIEW_BASELINE=tests/fixtures/lab-review/baseline DENPIE_LAB_REVIEW_CANDIDATE=tests/fixtures/lab-review/candidate sh scripts/build-frontend.sh >/dev/null && python3 -m http.server 3027 --bind 127.0.0.1 --directory frontend-astro/dist",
        url: "http://127.0.0.1:3027/lab-review",
        reuseExistingServer: false,
        timeout: 60_000,
    },
});
