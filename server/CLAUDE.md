# server

The Formal Notes account and sync API: Worker `formalnotes-api` on the custom domain `https://notes.formalsnake.dev`, Bun, Hono, Drizzle on Postgres through Hyperdrive, Better Auth with email and password and the bearer plugin. The contract is `../docs/protocol.md`, "Sync"; change it there first, never only here.

The server stores an email, Better Auth's hash of the derived password, the wrapped vault key and sealed blobs. It never logs a body, a blob, a token or an email: `onError` logs error names and Postgres codes only, Better Auth's logger and telemetry are off, sessions keep no IP or user agent, Workers invocation logs are off. No Sentry, no analytics.

## Commands

```sh
bun install
bun run test                # vitest in the Workers runtime, Postgres is PGlite started by test/global-setup.ts
bun run check               # tsc over src, test and scripts
bun run types               # after any wrangler.jsonc change
bunx wrangler deploy
bun scripts/roundtrip.ts    # the client side of the protocol against production; exits 1 on any mismatch
bun scripts/roundtrip.ts http://localhost:8787
```

`roundtrip.ts` reproduces `../docs/sync-vectors.json`, then signs up a throwaway `@example.com` account, syncs two devices, changes the password and deletes the account again. The desktop and mobile clients are checked against it, so a protocol change lands in the script too.

Deploys are manual with wrangler's own OAuth login. Never export `~/.claude/secrets/cloudflare-api-token` as `CLOUDFLARE_API_TOKEN` here: it has no Workers scopes.

## Secrets

`BETTER_AUTH_SECRET` only, set with `openssl rand -base64 48 | tr -d '\n' | bunx wrangler secret put BETTER_AUTH_SECRET`. Rotating it signs every device out. The Postgres password lives in the VM's `.env` and inside the Hyperdrive config `formalnotes-api` (`f21496b9e1b24ff6a79f5e14a32ef34f`, caching off), nowhere else. The Access service token for Hyperdrive (`formalnotes-hyperdrive`, expires 2027-11-10) is in `~/.claude/secrets/formalnotes-hyperdrive-access-token.json` on the macbook; renew it before then and update the Hyperdrive config with `bunx wrangler hyperdrive update f21496b9e1b24ff6a79f5e14a32ef34f --access-client-id ... --access-client-secret ...`.

## Migrations

Schema in `src/db/schema.ts`, SQL in `drizzle/`. `bun run db:generate` after a schema change, then apply over an SSH forward to the VM and close the forward by its pid:

```sh
ssh -f -N -o ExitOnForwardFailure=yes -L 55437:127.0.0.1:5437 vm-canarycoders
PID=$(lsof -nP -iTCP:55437 -sTCP:LISTEN -t | head -1)
DATABASE_URL="postgresql://formalnotes:$(ssh vm-canarycoders 'grep ^POSTGRES_PASSWORD= ~/srv/stacks/formalnotes/.env | cut -d= -f2-')@127.0.0.1:55437/formalnotes?sslmode=require" bun run db:migrate
kill "$PID"
```

## Database host

- `ssh vm-canarycoders`, stack `~/srv/stacks/formalnotes/`: `formalnotes-postgres` (postgres:17, SSL with the self-signed cert in `postgres-certs/`, `127.0.0.1:5437` inside the VM) and `formalnotes-pg-backup` (daily `pg_dump -Fc` to `backups/daily`, 14 kept, Sunday copies in `backups/weekly`, 8 kept). Secrets in the chmod 600 `.env` beside the compose file. The VM runs other products' production databases: touch only this stack.
- Ingress: the shared CanaryFarm tunnel has the rule `formalnotes.db.canarycoders.es` to `tcp://formalnotes-postgres:5432`; the `CloudFlare` connector reaches the container because it joined the `formalnotes-edge` network. Recreating the connector drops that join and the hostname answers 502 until `docker network connect formalnotes-edge CloudFlare` is redone. The tunnel config before this rule was added is `~/srv/stacks/formalnotes/canaryfarm-tunnel-config-202611100938.json`.
- The hostname is behind the Access app `formalnotes-hyperdrive` (service token above).
- Restore: `docker exec -i formalnotes-postgres pg_restore -U formalnotes -d formalnotes --clean < backups/daily/formalnotes-<day>.dump`.
