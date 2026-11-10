import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

export default defineConfig({
	plugins: [
		cloudflareTest(({ inject }) => ({
			wrangler: { configPath: "./wrangler.jsonc" },
			miniflare: {
				hyperdrives: { HYPERDRIVE: inject("databaseUrl") },
				bindings: { BETTER_AUTH_SECRET: "test-secret-that-is-at-least-32-characters" },
			},
		})),
	],
	test: { globalSetup: ["./test/global-setup.ts"] },
});
