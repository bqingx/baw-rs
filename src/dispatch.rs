//! Single guarded CLI dispatch. Batch cannot carry mutation authority.
use crate::{
    auth,
    cli::{Cli, Command, WalletCommand},
    client::Client,
    execution::{require_consent, OperationGuard},
    signing, trade,
};
use serde_json::{json, Value};

fn mutation(cli: &Cli) -> Result<Option<(&'static str, Value)>, String> {
    Ok(match &cli.command {
        Command::Auth { command } => Some((
            "auth",
            json!({"action": match command { auth::AuthCommand::Signin => "signin", auth::AuthCommand::Verify { .. } => "verify", auth::AuthCommand::Signout => "signout" }}),
        )),
        Command::MarketOrder { command } if trade::is_mutating(command) => {
            Some(("market-order.swap", trade::mutation_payload(command)?))
        }
        Command::Wallet {
            command: WalletCommand::Send(args),
        } => Some(("wallet.send", trade::send_payload(args)?)),
        Command::ContractCall { command } if signing::is_contract_mutating(command) => {
            Some(("contract-call.execute", signing::contract_intent(command)))
        }
        Command::SignMessage { command } if signing::is_message_mutating(command) => {
            Some(("sign-message.execute", signing::message_intent(command)))
        }
        _ => None,
    })
}
/// Must run before loading any wallet/session files.
pub fn preflight(cli: &Cli) -> Result<(), String> {
    let writes = matches!(
        &cli.command,
        Command::Auth { .. }
            | Command::Wallet {
                command: WalletCommand::Send(_)
            }
    ) || matches!(&cli.command, Command::MarketOrder { command } if trade::is_mutating(command))
        || matches!(&cli.command, Command::ContractCall { command } if signing::is_contract_mutating(command))
        || matches!(&cli.command, Command::SignMessage { command } if signing::is_message_mutating(command));
    if writes {
        require_consent(cli.confirm, cli.operation_id.as_deref())?;
    }
    if matches!(cli.command, Command::Batch)
        && (cli.confirm || cli.operation_id.is_some() || cli.accept_preview_sha256.is_some())
    {
        return Err("batch cannot authorize mutations or accept previews".into());
    }
    if matches!(&cli.command, Command::ContractCall { command } if signing::needs_accepted_digest(command))
        || matches!(&cli.command, Command::SignMessage { command } if signing::is_message_mutating(command))
    {
        let digest = cli
            .accept_preview_sha256
            .as_deref()
            .ok_or("signing execute requires --accept-preview-sha256")?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid accepted preview digest".into());
        }
    }
    mutation(cli)?;
    Ok(())
}
pub async fn execute(client: &mut Client, cli: &Cli) -> Result<Value, String> {
    preflight(cli)?;
    if client.credentials().session_id.is_empty()
        && !matches!(
            &cli.command,
            Command::Auth { .. }
                | Command::Wallet {
                    command: WalletCommand::Status
                }
        )
    {
        return Err("wallet session is missing".into());
    }
    let mut guard = if let Some((action, intent)) = mutation(cli)? {
        Some(OperationGuard::begin(
            &cli.wallet_dir,
            require_consent(cli.confirm, cli.operation_id.as_deref())?,
            action,
            &intent,
            &client.credentials().client_id,
        )?)
    } else {
        None
    };
    let result = match &cli.command {
        Command::Auth { command } => {
            auth::run(client, &cli.wallet_dir, &cli.instance_id, command).await
        }
        Command::Wallet {
            command: WalletCommand::Send(args),
        } => trade::send(client, args).await,
        Command::Wallet { command } => client.execute(command).await,
        Command::MarketOrder { command } => trade::run(client, command).await,
        Command::ContractCall { command } => {
            signing::contract(
                client,
                &cli.wallet_dir,
                command,
                cli.accept_preview_sha256.as_deref(),
            )
            .await
        }
        Command::SignMessage { command } => {
            signing::message(
                client,
                &cli.wallet_dir,
                command,
                cli.accept_preview_sha256.as_deref(),
            )
            .await
        }
        Command::Batch => Err("batch requires streaming dispatch".into()),
        Command::Serve => Err("serve runs before dispatch".into()),
    };
    if let Some(guard) = &mut guard {
        guard.finish(&result)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::os::unix::fs::PermissionsExt;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[tokio::test]
    async fn signed_out_signing_queries_are_rejected_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli::try_parse_from([
            "baw-rs",
            "--wallet-dir",
            dir.path().to_str().unwrap(),
            "--instance-id",
            "synthetic",
            "sign-message",
            "result",
            "--order-id",
            "synthetic-order",
        ])
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = Client::test_client(
            crate::session::Credentials {
                client_id: zeroize::Zeroizing::new("synthetic-client".into()),
                session_id: zeroize::Zeroizing::new(String::new()),
            },
            &format!("http://{}", listener.local_addr().unwrap()),
        )
        .unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            execute(&mut client, &cli),
        )
        .await;
        assert!(result.is_ok(), "signed-out query attempted network");
        assert!(result.unwrap().unwrap_err().contains("session is missing"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn send_dispatch_journals_before_http_preserves_precision_and_refuses_replay() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let amount = "0.000000000000000001000000000000000001";
        let cli = Cli::try_parse_from([
            "baw-rs",
            "--wallet-dir",
            dir.path().to_str().unwrap(),
            "--instance-id",
            "synthetic",
            "--confirm",
            "--operation-id",
            "send-precision",
            "wallet",
            "send",
            "--binanceChainId",
            "56",
            "--tokenAddress",
            "0x1111111111111111111111111111111111111111",
            "--recipient",
            "0x2222222222222222222222222222222222222222",
            "--amount",
            amount,
        ])
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let journal = dir.path().join(".baw-rs-operations/send-precision.json");
        let target = journal.clone();
        let server = tokio::spawn(async move {
            for i in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let record: Value =
                    serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
                assert_eq!(record["state"], "SUBMITTING");
                let mut bytes = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                if i == 0 {
                    assert!(request.starts_with("GET "));
                } else {
                    assert!(request.starts_with(
                        "POST /bapi/defi/v1/public/wallet-direct/agent-wallet/transfer "
                    ));
                    let body: Value =
                        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                    assert_eq!(body["amount"], amount);
                }
                let body=json!({"code":"000000","data":if i==0 {json!([])}else{json!({"txHash":"synthetic-hash"})}}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err(),
                "replayed mutation reached HTTP"
            );
        });
        let mut client = Client::test_client(
            crate::session::Credentials {
                client_id: zeroize::Zeroizing::new("synthetic-client".into()),
                session_id: zeroize::Zeroizing::new("synthetic-session".into()),
            },
            &origin,
        )
        .unwrap();
        assert_eq!(
            execute(&mut client, &cli).await.unwrap()["status"],
            "SUBMITTED"
        );
        assert!(execute(&mut client, &cli)
            .await
            .unwrap_err()
            .contains("existing operation-id"));
        server.await.unwrap();
        let text = std::fs::read_to_string(journal).unwrap();
        assert!(text.contains("synthetic-hash"));
        assert!(
            !text.contains("synthetic-session")
                && !text.contains("synthetic-client")
                && !text.contains(amount)
        );
    }
}
