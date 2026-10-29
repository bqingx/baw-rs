# Native authentication (v0.2)

Commands: `auth signin`, `auth verify --qrCodeId ID`, `auth signout`. Select `--wallet-dir` and `--instance-id` explicitly for every invocation. Each auth command additionally requires `--confirm --operation-id UNIQUE_ID`; verify uses a different operation ID from signin. Consent is checked before opening or creating the directory. No development/test invocation used a live session or production backend.

## Signin and verify

Signin creates only the selected missing wallet subdirectory (0700) and encrypted client identity, or loads that directory's existing stable identity. It never regenerates a malformed existing file. For an existing session it queries login status; already CONNECTED+CREATED returns ALREADY_CONNECTED. A pending wallet creation stops rather than replacing it. An expired/disconnected session can start an explicitly authorized fresh signin without forwarding its old cookie. An unexpired local pending QR is not replaced.

The login request generates an ephemeral secp256k1 public key and sends the compressed key without its prefix, as the installed protocol requires. No local wallet transaction/message signing is implemented. The returned QR URL, QR ID, expiration and pairing code are output as JSON; no browser/image process is launched. Pending cookie/client data is encrypted in the selected directory's private `auth-pending` file, not exposed in JSON. The original session stays unchanged until verification completes.

Verify must match the selected client, session-file baseline and QR ID. It loads the staged cookie and polls confirm until CONNECTED, then query until CONNECTED+CREATED. Polling has a two-second interval and is bounded by the original server QR expiration and five minutes measured from signin; restarting verify does not reset that cap. Session commit refuses a changed baseline. Cookie adoption is only after a successful HTTP/API envelope. No automatic retry of failed HTTP submits occurs; verify polling is an explicit protocol operation, not write resubmission.

## Storage

Encrypted file format is compatible with installed BAW: PBKDF2-HMAC-SHA256, 100000 iterations, salt `baw-agentic-wallet-v1:<instance>`, AES-256-GCM with 16-byte IV, base64(IV+ciphertext+tag). Both fields are encrypted on native auth save. Legacy plaintext client UUIDs are accepted; query loading never migrates them. File persistence uses a fresh private file, fsync, atomic same-directory rename and directory fsync; auth directory locking and session-baseline checks prevent stale commits. Files require 0600 owner-only, regular, single-link storage; wallet directories require 0700 and no traversed symlinks. Wrong identity, malformed JSON/ciphertext, unsafe modes or links fail closed without repair.

Auth writes, pending state and preview/journal files are directory-local. There is no global keyring integration. An authenticated encrypted empty session is the intentional logged-out native state: `wallet status` returns UNCONNECTED without networking; other commands reject a missing session. Missing/malformed session files are not silently interpreted as logout or a different wallet.

## Signout and recovery

Signout attempts the bodyless remote logout once, invalidates memory, atomically clears the local session while preserving client identity, then removes pending auth. If remote logout fails, the command reports `local session cleared; server logout uncertain` rather than claiming verified server logout. Local persistence failure is explicitly reported as possibly completed remotely. No auth operation automatically removes its durable operation journal, and duplicate operation IDs never resubmit.

Offline tests cover separate signin/verify invocations, cookie rotation, private encrypted persistence/atomic inode replacement, original-compatible loading, stale commits, expired pending flows, bounded polling, signout cleanup/uncertainty, fresh signin after expiry and credential-free outputs. Live QR scanning, real backend device policy and real cross-client session behavior remain unverified by this migration.
