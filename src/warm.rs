//! Optional resident mode: one decrypted session, one connection pool, cached
//! developer-mode/wallet probe. The socket lives in the selected wallet
//! directory (0700), is owner-only (0600) and rejects connections from a
//! different uid. Every request is still executed through the normal guarded
//! dispatch; only the transport and the wallet probe are reused.
use crate::cli::{Cli, Command};
use crate::client::Client;
use crate::session;
use clap::Parser;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join(".baw-rs.sock")
}

fn prepare_socket(path: &Path) -> Result<UnixListener, String> {
    let dir = path.parent().ok_or("invalid wallet directory")?;
    let meta = std::fs::metadata(dir).map_err(|_| "cannot inspect wallet directory")?;
    if meta.permissions().mode() & 0o777 != 0o700 {
        return Err("wallet directory must be private (0700)".into());
    }
    if path.exists() {
        // A stale socket from a crashed daemon is replaced; a live one is detected by connect.
        if UnixStream::connect(path).is_ok() {
            return Err("warm server already running for this wallet".into());
        }
        std::fs::remove_file(path).map_err(|_| "cannot remove stale socket")?;
    }
    let listener = UnixListener::bind(path).map_err(|_| "cannot bind warm socket")?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot restrict warm socket")?;
    Ok(listener)
}

pub async fn serve(cli: &Cli) -> Result<Value, String> {
    let credentials = session::load(&cli.wallet_dir, &cli.instance_id)?;
    if credentials.session_id.is_empty() {
        return Err("warm server requires a connected wallet".into());
    }
    let mut client = Client::new(credentials)?;
    let path = socket_path(&cli.wallet_dir);
    let listener = prepare_socket(&path)?;
    // The probe cache is only enabled in this process; a normal CLI keeps the
    // strict per-command settings/address verification.
    std::env::set_var("BAW_RS_WARM", "1");
    eprintln!("warm server listening on {}", path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if let Err(message) = handle(&mut client, &cli.wallet_dir, &cli.instance_id, stream).await {
            eprintln!("warm request failed: {message}");
        }
    }
    Ok(json!({"status": "stopped"}))
}

async fn handle(
    client: &mut Client,
    dir: &Path,
    instance: &str,
    stream: UnixStream,
) -> Result<(), String> {
    if !same_uid(&stream)? {
        return Err("warm socket peer uid mismatch".into());
    }
    let mut writer = stream.try_clone().map_err(|_| "cannot clone warm socket")?;
    let reader = BufReader::new(stream);
    let mut line = String::new();
    {
        let mut reader = reader;
        if reader
            .read_line(&mut line)
            .map_err(|_| "cannot read warm request")?
            == 0
        {
            return Ok(());
        }
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(v) => match v["argv"].as_array() {
            Some(argv) if !argv.is_empty() && argv.len() <= 64 => {
                let args: Option<Vec<String>> =
                    argv.iter().map(|a| a.as_str().map(str::to_owned)).collect();
                match args {
                    Some(mut args) => {
                        let mut full = vec![
                            "baw-rs".to_owned(),
                            "--wallet-dir".to_owned(),
                            dir.to_string_lossy().into_owned(),
                            "--instance-id".to_owned(),
                            instance.to_owned(),
                        ];
                        full.append(&mut args);
                        match Cli::try_parse_from(full) {
                            Ok(parsed) => match parsed.command {
                                // Never accept control-plane or streaming commands over the socket.
                                Command::Serve | Command::Batch => {
                                    json!({"success": false, "error": {"message": "command not allowed in warm mode"}})
                                }
                                _ => match crate::dispatch::execute(client, &parsed).await {
                                    Ok(data) => json!({"success": true, "data": data}),
                                    Err(message) => {
                                        json!({"success": false, "error": {"message": message}})
                                    }
                                },
                            },
                            Err(_) => {
                                json!({"success": false, "error": {"message": "invalid warm command"}})
                            }
                        }
                    }
                    None => json!({"success": false, "error": {"message": "argv must be strings"}}),
                }
            }
            _ => json!({"success": false, "error": {"message": "argv must be a non-empty array"}}),
        },
        Err(_) => json!({"success": false, "error": {"message": "invalid warm request JSON"}}),
    };
    let mut body = serde_json::to_vec(&response).map_err(|_| "cannot encode warm response")?;
    body.push(b'\n');
    writer
        .write_all(&body)
        .map_err(|_| "cannot write warm response")?;
    writer.flush().map_err(|_| "cannot flush warm response")?;
    Ok(())
}

fn same_uid(stream: &UnixStream) -> Result<bool, String> {
    use std::os::unix::io::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err("cannot read warm socket peer credentials".into());
    }
    Ok(cred.uid == unsafe { libc::geteuid() })
}

/// Client side: hand one argv vector to the resident server. Any transport
/// failure returns None so the caller can fall back to a normal cold call.
pub fn send(dir: &Path, argv: &[String]) -> Option<Value> {
    let path = socket_path(dir);
    if !path.exists() {
        return None;
    }
    let mut stream = UnixStream::connect(path).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .ok()?;
    let mut request = serde_json::to_vec(&json!({"argv": argv})).ok()?;
    request.push(b'\n');
    stream.write_all(&request).ok()?;
    stream.flush().ok()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}
