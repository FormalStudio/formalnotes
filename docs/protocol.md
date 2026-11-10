# Files and sync

What the desktop app, the mobile app and the server agree on. Test vectors for every crypto
step are in [sync-vectors.json](sync-vectors.json); each implementation has a test that
reproduces them byte for byte.

## Files in a library

A library is a folder. Paths are relative to it, `/`-separated, NFC, never starting with `/`
and never containing `..`.

- A note is a `.md` file. Its title is the first `# ` heading after the properties block, or
  the file name without `.md`.
- A folder is a directory. Dot directories (`.trash`, anything else starting with `.`) are not
  part of the library.
- `.folder.json` in a folder holds that folder's view. No file means the cards view.

### Properties

A note may start with a properties block: a first line of exactly `---`, lines of
`name: value`, and a closing line of exactly `---`. It is the YAML front matter Obsidian calls
properties.

```markdown
---
status: Doing
due: 2026-11-14
tags: [work, q4]
done: false
---
# Plan the release
```

| Type | Written as |
| --- | --- |
| `text`, `select`, `url` | the string, double-quoted when it would not read back as a string (`"true"`, `"12"`, leading or trailing space, a `:` followed by a space, a leading `[`, `"`, `#` or `-`) |
| `number` | `12`, `-3.5` |
| `checkbox` | `true`, `false` |
| `date` | `2026-11-14` |
| `multi_select` | `[one, two]`, items quoted by the same rule, or when they hold a `,` or `]` |

An empty value removes the line. A block with no lines left is removed with its fences.
Writing one property touches only that property's line: every other line, including YAML this
app does not model (nested maps, block lists, comments), stays byte for byte.

### .folder.json

```json
{
  "v": 1,
  "view": "table",
  "columns": [
    { "name": "status", "type": "select", "options": ["Todo", "Doing", "Done"], "width": 140 },
    { "name": "due", "type": "date" }
  ],
  "sort": { "column": "modified", "descending": true }
}
```

`view` is `cards` or `table`. A column's `name` is the property name in the notes. `type` is
one of the types above; `options` belongs to `select` and `multi_select`; `width` is optional
points. `title`, `created` and `modified` are built-in columns: they are not listed in
`columns`, `title` always comes first, and they are valid in `sort.column`. Unknown keys are
kept when the file is rewritten.

## Sync

The server stores opaque encrypted objects and an account. It never sees a note, a file name,
a folder name, the password, or a key.

### Keys

All from the password, on the device:

1. `masterKey = PBKDF2-HMAC-SHA256(password, salt, 600000 iterations, 32 bytes)` with
   `salt = UTF-8("formalnotes:v1:" + email.trim().toLowerCase())`.
2. `authKey = HKDF-SHA256(ikm = masterKey, salt = empty, info = "formalnotes auth v1", 32 bytes)`.
   The account password the server sees is `base64url(authKey)` without padding. The server
   hashes it again.
3. `wrapKey = HKDF-SHA256(ikm = masterKey, salt = empty, info = "formalnotes wrap v1", 32 bytes)`.
4. `vaultKey` is 32 random bytes made at sign-up. Every object is encrypted with it. The
   server keeps it only as `wrappedVaultKey = seal(wrapKey, vaultKey, aad = "vault-key")`.

`seal(key, plaintext, aad)` is AES-256-GCM with a fresh random 12-byte nonce, laid out as
`0x01 || nonce || ciphertext || 16-byte tag`. On the wire it is standard base64 with padding.
`open` rejects any other first byte.

A device keeps the session token and the vault key in the OS keychain after sign-in. The
password is never stored. There is no password reset: the server cannot re-wrap a key it
cannot read. Changing the password re-wraps the vault key under the new `wrapKey`.

### Objects

One object per note, per folder and per `.folder.json`. Its id is a random UUID v4, lowercase
with hyphens, made by the device that first uploads it. The blob is
`seal(vaultKey, padded, aad = id)` where `padded` is this JSON in UTF-8, followed by spaces
(0x20) up to the next multiple of 1024 bytes:

```json
{ "v": 1, "kind": "note", "path": "Work/Plan.md", "text": "...", "created": 1794300000000, "modified": 1794303600000 }
```

`kind` is `note`, `folder` or `file` (`file` is a `.folder.json`). `text` is the whole file
and is absent for a folder. `created` and `modified` are Unix milliseconds. Unknown keys are
ignored. A device applying an object sets the file's modification time to `modified`.

### API

Base URL `https://notes.formalsnake.dev`. JSON in and out. Every `/v1` route needs
`Authorization: Bearer <session token>`. Errors are `{ "error": "<code>", "message": "..." }`
with a 4xx or 5xx status; `401 unauthorized` means sign in again.

Account, through Better Auth with the bearer plugin (the session token is the
`set-auth-token` response header):

| Route | Body | Result |
| --- | --- | --- |
| `POST /api/auth/sign-up/email` | `{ name, email, password }`, `name` is the email's local part | session |
| `POST /api/auth/sign-in/email` | `{ email, password }` | session |
| `POST /api/auth/sign-out` | | |

`password` is always `base64url(authKey)`.

| Route | Body | Result |
| --- | --- | --- |
| `GET /v1/account` | | `{ email, objects, bytes, avatar }`: live objects, stored bytes, the picture's version or `null` |
| `GET /v1/keys` | | `{ wrappedVaultKey }`, or `404 no_keys` |
| `PUT /v1/keys` | `{ wrappedVaultKey }` | `204`; `409 keys_exist` once set |
| `POST /v1/account/password` | `{ currentPassword, newPassword, wrappedVaultKey }` | `204`; other sessions end |
| `DELETE /v1/account` | `{ password }` | `204`; the user, sessions, keys and objects are gone |
| `GET /v1/avatar` | | `{ avatar, blob }`, or `404 no_avatar` |
| `PUT /v1/avatar` | `{ blob }` | `{ avatar }`, the new version |
| `DELETE /v1/avatar` | | `204` |
| `GET /v1/sync?since=<seq>&limit=<n>` | | `{ changes, cursor, more }` |
| `POST /v1/sync` | `{ changes }` | `{ results }` |
| `GET /health` | | `{ ok: true }`, no auth |

Sign-up is two calls: `sign-up/email`, then `PUT /v1/keys`. Sign-in is `sign-in/email`, then
`GET /v1/keys` and unwrap. An account whose `GET /v1/keys` answers `no_keys` (sign-up was cut
short) makes a vault key and puts it.

**Profile picture.** `blob` is `seal(vaultKey, jpeg, aad = "avatar")`, where `jpeg` is a square
JPEG 512 pixels across, at most 262144 bytes sealed (`413 too_large`). The server cannot open
it. `avatar` is the Unix milliseconds of the last upload: a device keeps the opened picture
with that number and fetches again when `GET /v1/account` reports another. An account without
a picture shows the first letter of its email.

**Pull.** Every account has a counter. Each accepted change takes the next value as the
object's `seq`. `GET /v1/sync?since=0` returns every object; `since=<cursor>` returns the
objects whose `seq` is greater, in `seq` order, at most `limit` (default and maximum 500):

```json
{ "changes": [{ "id": "…", "seq": 42, "deleted": false, "blob": "…" }], "cursor": 42, "more": false }
```

An object appears once, in its current state. A deleted object has `"blob": null`. `cursor`
is the last `seq` returned, or `since` when nothing changed. A device stores `cursor` and
pulls again while `more` is true. The cursor moves only on a pull, never from a push result.

**Push.** At most 100 changes a request, each blob at most 1 MiB decoded:

```json
{ "changes": [{ "id": "…", "base": 41, "deleted": false, "blob": "…" }] }
```

`base` is the `seq` this device last saw for the object, `0` for one it is creating. The
server takes a change when the object's current `seq` equals `base` (or the object does not
exist and `base` is `0`), and answers in the same order:

```json
{ "results": [
  { "id": "…", "status": "ok", "seq": 43 },
  { "id": "…", "status": "conflict", "seq": 42, "deleted": false, "blob": "…" }
] }
```

A conflict carries the server's current state. Taken changes are kept even when others in the
request conflict. A change with a `base` above 0 for an id the server has never seen answers
`conflict` with `"seq": 0, "deleted": true, "blob": null`.

Limits: 20000 live objects and 200,000,000 bytes of blobs an account (`413 too_large` for a
blob, `403 quota` for the account), and `429 rate_limited`. `too_large`, `quota` and a
malformed change (`400 bad_request`) reject the whole request and keep nothing; only
conflicts are answered one by one.

Other codes: `403 wrong_password` from `POST /v1/account/password` and `DELETE /v1/account`;
Better Auth's own errors in the same shape with its code lowercased
(`invalid_email_or_password`, `user_already_exists_use_another_email`, `password_too_short`). The account
password must be exactly 43 base64url characters. `limit` above 500 is read as 500.

### What a device does

It keeps, per object: `id`, `path`, the SHA-256 of what it last synced, and that `seq`.

- **Local change.** A file that differs from its synced hash is pushed with `base` = its
  `seq`. A new file gets a new id and `base` 0. A file that is gone is pushed as deleted. A
  file that vanished while one with the same content appeared elsewhere is a move: same id,
  new path.
- **Remote change.** Applied to disk when the local file still matches its synced hash. A
  remote delete moves the local file to the trash.
- **Conflict** (a push answered `conflict`, or a pulled change for a file with unsynced
  edits): decrypt the server's state.
  - Same kind, path and text as the local one: take the server's `seq`. This is also how a
    push whose answer was lost settles.
  - Server deleted, local edited: push again with `base` = the server's `seq`.
  - Local deleted, server edited: restore the server's version.
  - Both edited: the server's version takes the file. The local text becomes a new note
    beside it named `<name> (conflict YYYY-MM-DD HH.mm).md`, pushed as a new object.
- **Same path, two ids** (two devices made `Untitled.md` offline): the one that arrives
  second is written as `<name> 2.md` and that rename is pushed.
- **First sync of a library that already has files**: pull everything, adopt the id of every
  remote object whose path and text match a local file, then treat the rest as above.

A device syncs two seconds after a local change, when its window or app comes to the front,
and every 30 seconds while it is open.
