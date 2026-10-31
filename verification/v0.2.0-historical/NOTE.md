# Quarantined: v0.2.0 evidence

These files were shipped inside the v0.2.2 source drop. They belong to a **different
tree** and do not describe the source in this repository.

Why that conclusion holds (recomputed, not assumed):

- `source-sha256.json` recorded 19 files; **9 do not match** the shipped source
  (`Cargo.toml`, `Cargo.lock`, `src/auth.rs`, `src/cli.rs`, `src/client.rs`,
  `src/dispatch.rs`, `src/lib.rs`, `src/main.rs`, `src/signing.rs`).
- Every log reads `Compiling baw-rs v0.2.0 (<repo>)`, while `Cargo.toml` shipped
  as `0.2.2`. Different version, different working directory.
- The manifest's test tally (100 library tests) differs from what this tree runs
  (105 library tests, 114 total), which is consistent with tests added after those
  gates ran.
- No `dist/baw-rs` binary was shipped, so the executable SHA-256 and byte size quoted
  in `INTEGRATION-REPORT.md` are unverifiable from the drop.

Nothing here should be cited as validation of the current source. It is retained only
for history. Current, reproducible evidence lives one directory up and is produced by
`scripts/verify.sh`.

`VERIFIED-BY-HERMES-v0.2.2.log` is an independent reviewer run on the shipped tree,
kept here because it was produced before this repository existed.
