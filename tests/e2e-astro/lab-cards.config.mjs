import { existsSync } from "node:fs";
import { defineConfig, devices } from "@playwright/test";

const chromiumExecutable = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
].find((candidate) => candidate !== undefined && existsSync(candidate));

// The lab page is static and needs no database or backend. A foreground static
// server keeps this browser check offline and binds only the agent-owned port.
export default defineConfig({
    testDir: ".",
    testMatch: "lab-cards.spec.ts",
    fullyParallel: false,
    forbidOnly: Boolean(process.env.CI),
    retries: 0,
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
            "python3 -m http.server 3027 --bind 127.0.0.1 --directory ../../frontend-astro/dist",
        url: "http://127.0.0.1:3027/lab-cards",
        reuseExistingServer: true,
        timeout: 60_000,
    },
});
