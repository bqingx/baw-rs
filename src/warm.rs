//! Backward-compatible single-wallet CLI using the same bounded native resident engine.
use crate::cli::{envelope, Cli};
use serde_json::Value;
use std::path::{Path, PathBuf};
pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join(".baw-rs.sock")
}
pub async fn serve(cli: &Cli) -> Result<Value, String> {
    crate::resident::serve_legacy(cli).await
}
/// Preserve cold fallback only when no socket exists. Once IPC may have submitted a
/// mutation, an error is final: never retry a possibly completed operation cold.
pub async fn send(dir: &Path, argv: &[String]) -> Option<Value> {
    let path = socket_path(dir);
    if std::fs::symlink_metadata(&path).is_err() {
        return None;
    }
    Some(envelope(
        crate::resident::send(
            &path,
            crate::resident::Request::Run {
                name: "single".into(),
                argv: argv.to_vec(),
            },
        )
        .await,
    ))
}
