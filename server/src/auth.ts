import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { bearer } from "better-auth/plugins/bearer";
import type { Database } from "./db";
import { schema } from "./db";

/** `base64url(authKey)` without padding: 32 bytes are always 43 characters. */
export const PASSWORD_LENGTH = 43;

export function createAuth(db: Database, env: Env) {
	return betterAuth({
		appName: "Formal Notes",
		baseURL: env.BETTER_AUTH_URL,
		secret: env.BETTER_AUTH_SECRET,
		database: drizzleAdapter(db, { provider: "pg", schema }),
		emailAndPassword: {
			enabled: true,
			minPasswordLength: PASSWORD_LENGTH,
			maxPasswordLength: PASSWORD_LENGTH,
		},
		plugins: [bearer()],
		session: {
			// Devices keep the token in the keychain and never see the password again, so a session lasts.
			expiresIn: 60 * 60 * 24 * 365,
			updateAge: 60 * 60 * 24,
		},
		// Sign-in and sign-up are rate limited per IP by the Worker's rate limit bindings; Better Auth's limiter
		// would only count inside one isolate.
		rateLimit: { enabled: false },
		// The server keeps an email and ciphertext. No addresses, no user agents, no log lines that could hold either.
		advanced: { ipAddress: { disableIpTracking: true } },
		logger: { disabled: true },
		telemetry: { enabled: false },
		databaseHooks: {
			session: {
				create: {
					before: async (session) => ({ data: { ...session, ipAddress: null, userAgent: null } }),
				},
			},
		},
	});
}

export type Auth = ReturnType<typeof createAuth>;
