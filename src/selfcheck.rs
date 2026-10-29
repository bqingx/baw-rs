//! Local self-checks the JavaScript CLI ships: `cli-check` and `skill-check`.
//!
//! Neither leaf belongs to the `agent-wallet` API family, so neither goes through
//! [`Client`]'s `request` (that origin is fixed to Binance and must not be used for
//! third-party hosts). `skill-check` fetches one published markdown document from the
//! skills hub through a private, tightly constrained `reqwest` client defined here.
//!
//! Bundle ground truth (`@binance/agentic-wallet/dist/index.js`, 1.12.0):
//!
//! ```text
//! function dt(t){return t.split("-")[0]}
//! function Ft(t,e){let n=dt(t).split(".").map(Number),r=dt(e).split(".").map(Number);
//!   for(let o=0;o<Math.max(n.length,r.length);o++){let s=n[o]||0,i=r[o]||0;
//!   if(s>i)return!0;if(s<i)return!1}return!1}
//! var Ys=Ks(import.meta.url),Xs=Ys("../package.json"),Ee=Xs.version;
//! ```
//!
//! `Ft(candidate, current)` answers "is `candidate` newer than `current`"; `dt` drops a
//! prerelease suffix; `Ee` is the installed CLI's own package version.
use crate::client::Client;
use serde_json::{json, Value};
use std::time::Duration;

// Bundle: `Xs=Ys("../package.json"),Ee=Xs.version` — `@binance/agentic-wallet`
// package.json declares `"version": "1.12.0"`. `cli-check` compares against the CLI's
// *own* version, so the port reports the wire-compatible 1.12.0 exactly like
// `client.rs` pins `version_code`/`clientVersion`: a skill requiring `>= 1.12.0` must not
// be told to "update" a binary that already speaks that protocol.
const CLI_VERSION: &str = "1.12.0";

// Bundle (skill-check):
// `let i=`https://raw.githubusercontent.com/binance/binance-skills-hub/refs/heads/main/skills/binance-web3/${o}/SKILL.md``
const SKILLS_BASE: &str =
    "https://raw.githubusercontent.com/binance/binance-skills-hub/refs/heads/main";

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_DOC: usize = 64 * 1024;

/// Static, sanitized: never echoes a host, a status body or backend text.
const ERR_FETCH: &str = "skill-check: could not reach the skill metadata registry";
const ERR_DOC: &str = "skill-check: skill metadata is unusable";
const ERR_SKILL_NAME: &str = "skill-check: invalid skill name";
const ERR_VERSION: &str = "invalid version: expected dotted numeric segments";

#[derive(clap::Args, Debug)]
pub struct CliCheckArgs {
    /// Required CLI version
    #[arg(long = "required-version")]
    pub required_version: String,
}

#[derive(clap::Args, Debug)]
pub struct SkillCheckArgs {
    /// Skill name to check
    #[arg(long = "skill-name")]
    pub skill_name: String,
    /// Current version to compare against
    #[arg(long = "current-version")]
    pub current_version: String,
}

/// `baw cli-check --required-version <version>` — purely local, no network.
///
/// Bundle: `let s=Ft(o,Ee);p()?g({currentCliVersion:Ee,needUpdateCli:s}):...` where `o`
/// is `--required-version` and `Ee` the installed CLI version. The bundle contains no
/// publication lookup for this leaf at all (`registry.npmjs.org` never appears), so no
/// registry fetch is implemented here on purpose.
pub async fn cli_check(_client: &mut Client, args: &CliCheckArgs) -> Result<Value, String> {
    cli_check_against(CLI_VERSION, &args.required_version)
}

fn cli_check_against(current: &str, required: &str) -> Result<Value, String> {
    let need_update = is_newer(required, current)
        .ok_or_else(|| format!("cli-check: --required-version is not a version\n{ERR_VERSION}"))?;
    // Bundle JSON shape: `{currentCliVersion: Ee, needUpdateCli: s}`.
    Ok(json!({
        "currentCliVersion": current,
        "needUpdateCli": need_update,
    }))
}

/// `baw skill-check --skill-name <name> --current-version <version>`.
///
/// Bundle: fetch the skill's `SKILL.md` from the skills hub, extract the frontmatter
/// `version:` with `Ed` (`/^\s*version:\s*["']?([^"'\s]+)["']?\s*$/m`) and answer
/// `Ft(latest, current)` as `{needUpdateSkill, latestSkillVersion}`.
///
/// Deliberate divergence from the reference: on any failure the bundle prints
/// `{needUpdateSkill:false, latestSkillVersion:""}` (it fails *open*, i.e. "you are
/// current"). A check that cannot read the registry must never claim "up to date", so
/// every failure here is a static error instead.
pub async fn skill_check(_client: &mut Client, args: &SkillCheckArgs) -> Result<Value, String> {
    skill_check_at(SKILLS_BASE, &args.skill_name, &args.current_version).await
}

async fn skill_check_at(base: &str, skill_name: &str, current: &str) -> Result<Value, String> {
    let doc = fetch_skill_doc(base, skill_name).await?;
    let latest = parse_skill_version(&doc).ok_or_else(|| ERR_DOC.to_string())?;
    let need_update = is_newer(&latest, current)
        .ok_or_else(|| format!("skill-check: --current-version is not a version\n{ERR_VERSION}"))?;
    // Bundle JSON shape: `g({needUpdateSkill:u,latestSkillVersion:c})`.
    Ok(json!({
        "needUpdateSkill": need_update,
        "latestSkillVersion": latest,
    }))
}

async fn fetch_skill_doc(base: &str, skill_name: &str) -> Result<String, String> {
    if !valid_skill_name(skill_name) {
        return Err(ERR_SKILL_NAME.to_string());
    }
    let url = reqwest::Url::parse(&format!("{base}/skills/binance-web3/{skill_name}/SKILL.md"))
        .map_err(|_| ERR_FETCH.to_string())?;
    if !url_allowed(url.scheme(), url.host_str()) {
        return Err(ERR_FETCH.to_string());
    }
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(FETCH_TIMEOUT)
        .no_proxy()
        .build()
        .map_err(|_| ERR_FETCH.to_string())?;
    let mut response = http
        .get(url)
        .send()
        .await
        .map_err(|_| ERR_FETCH.to_string())?;
    if !response.status().is_success() {
        return Err(ERR_FETCH.to_string());
    }
    if response.content_length().unwrap_or(0) > MAX_DOC as u64 {
        return Err(ERR_DOC.to_string());
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ERR_FETCH.to_string())? {
        if body.len() + chunk.len() > MAX_DOC {
            return Err(ERR_DOC.to_string());
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| ERR_DOC.to_string())
}

/// HTTPS only. Tests may bind loopback plain HTTP; that is the only exception and it is
/// compiled out of every non-test build.
fn url_allowed(scheme: &str, host: Option<&str>) -> bool {
    if scheme == "https" {
        return true;
    }
    #[cfg(test)]
    if scheme == "http" && matches!(host, Some("127.0.0.1" | "[::1]")) {
        return true;
    }
    let _ = host;
    false
}

/// A skill name becomes a path segment, so it must not carry `/`, `..` or whitespace.
fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Bundle: `function dt(t){return t.split("-")[0]}` — the prerelease suffix is dropped.
/// Numeric segments only; anything else is a hard error, because the reference silently
/// turns such input into `NaN` comparisons ("not newer") and we refuse to report that as
/// "up to date".
fn parse_version(version: &str) -> Option<Vec<u64>> {
    let core = version.split('-').next()?;
    if core.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    for segment in core.split('.') {
        if segment.is_empty() || segment.len() > 18 || !segment.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        segments.push(segment.parse::<u64>().ok()?);
    }
    Some(segments)
}

/// Bundle `Ft`: segment-wise numeric compare, missing segments are `0`, so `1.10 > 1.9`
/// and `2.0.0 > 1.99.99`. Returns `Some(true)` when `candidate` is newer than `current`.
fn is_newer(candidate: &str, current: &str) -> Option<bool> {
    let newer = parse_version(candidate)?;
    let installed = parse_version(current)?;
    for index in 0..newer.len().max(installed.len()) {
        let left = newer.get(index).copied().unwrap_or(0);
        let right = installed.get(index).copied().unwrap_or(0);
        if left > right {
            return Some(true);
        }
        if left < right {
            return Some(false);
        }
    }
    Some(false)
}

/// Bundle `Ed`: `/^\s*version:\s*["']?([^"'\s]+)["']?\s*$/m`, applied per line.
fn parse_skill_version(doc: &str) -> Option<String> {
    for line in doc.lines() {
        let rest = match line.trim_start().strip_prefix("version:") {
            Some(rest) => rest.trim_start(),
            None => continue,
        };
        let rest = match rest.strip_prefix('"').or_else(|| rest.strip_prefix('\'')) {
            Some(quoted) => quoted,
            None => rest,
        };
        let mut parts = rest.splitn(2, |c: char| c == '"' || c == '\'' || c.is_whitespace());
        let value = parts.next().unwrap_or("");
        let trailing = parts.next().unwrap_or("");
        let trailing = trailing.trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
        if value.is_empty() || !trailing.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::thread::JoinHandle;

    fn ok_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/markdown\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    /// Loopback server that answers exactly one request and hands back the request head.
    fn serve(response: String) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (base, rx, thread)
    }

    /// A port that is bound and immediately released: connections are refused.
    fn dead_base() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn cli_check_newer_required() {
        let value = cli_check_against("1.12.0", "1.13.0").unwrap();
        assert_eq!(value["needUpdateCli"], json!(true));
        assert_eq!(value["currentCliVersion"], json!("1.12.0"));
    }

    #[tokio::test]
    async fn cli_check_equal_version() {
        let value = cli_check_against("1.12.0", "1.12.0").unwrap();
        assert_eq!(value["needUpdateCli"], json!(false));
        assert_eq!(value["currentCliVersion"], json!("1.12.0"));
    }

    #[tokio::test]
    async fn cli_check_older_required() {
        let value = cli_check_against("1.12.0", "1.11.0").unwrap();
        assert_eq!(value["needUpdateCli"], json!(false));
    }

    #[tokio::test]
    async fn cli_check_malformed_input_fails_closed() {
        let error = cli_check_against("1.12.0", "latest").unwrap_err();
        assert!(error.contains("cli-check"), "{error}");
        assert!(!error.to_lowercase().contains("up to date"));
        assert!(cli_check_against("1.12.0", "").is_err());
        assert!(cli_check_against("1.12.0", "1..2").is_err());
        assert!(cli_check_against("1.12.0", "1.x.0").is_err());
    }

    #[test]
    fn version_ordering_is_numeric_not_lexicographic() {
        assert_eq!(is_newer("1.10.0", "1.9.0"), Some(true));
        assert_eq!(is_newer("1.9.0", "1.10.0"), Some(false));
        assert_eq!(is_newer("2.0.0", "1.99.99"), Some(true));
        assert_eq!(is_newer("1.99.99", "2.0.0"), Some(false));
        assert_eq!(is_newer("1.12.0", "1.12"), Some(false));
        assert_eq!(is_newer("1.12.1", "1.12"), Some(true));
        // `dt` drops the prerelease suffix before comparing.
        assert_eq!(is_newer("1.13.0-rc.2", "1.12.0"), Some(true));
        assert_eq!(is_newer("1.12.0-beta.1", "1.12.0"), Some(false));
    }

    #[tokio::test]
    async fn skill_check_reports_newer_skill() {
        let (base, requests, server) = serve(ok_response(
            "---\nname: binance-web3\ndescription: demo\nversion: \"1.3.0\"\n---\n\n# Body\n",
        ));
        let value = skill_check_at(&base, "binance-web3", "1.2.0")
            .await
            .unwrap();
        let request = requests.recv().unwrap();
        let _ = server.join();
        assert!(
            request.starts_with("GET /skills/binance-web3/binance-web3/SKILL.md "),
            "{request}"
        );
        assert_eq!(value["needUpdateSkill"], json!(true));
        assert_eq!(value["latestSkillVersion"], json!("1.3.0"));
    }

    #[tokio::test]
    async fn skill_check_current_skill_is_not_newer() {
        let (base, requests, server) = serve(ok_response("---\nversion: 1.3.0\n---\n"));
        let value = skill_check_at(&base, "binance-web3", "1.3.0")
            .await
            .unwrap();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(value["needUpdateSkill"], json!(false));
        assert_eq!(value["latestSkillVersion"], json!("1.3.0"));
    }

    #[tokio::test]
    async fn skill_check_fails_closed_when_registry_unreachable() {
        let error = skill_check_at(&dead_base(), "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        assert_eq!(error, ERR_FETCH);
    }

    #[tokio::test]
    async fn skill_check_fails_closed_on_error_status() {
        let (base, requests, server) = serve(
            "HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\nconnection: close\r\n\r\nnot found"
                .to_string(),
        );
        let error = skill_check_at(&base, "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(error, ERR_FETCH);
        assert!(!error.contains("not found"), "backend text leaked: {error}");
    }

    #[tokio::test]
    async fn skill_check_fails_closed_on_redirect() {
        let (base, requests, server) = serve(
            "HTTP/1.1 302 Found\r\nlocation: https://example.invalid/SKILL.md\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                .to_string(),
        );
        let error = skill_check_at(&base, "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(error, ERR_FETCH);
    }

    #[tokio::test]
    async fn skill_check_rejects_oversized_document() {
        let doc = format!("---\nversion: 1.3.0\n---\n{}\n", "x".repeat(MAX_DOC));
        let (base, requests, server) = serve(ok_response(&doc));
        let error = skill_check_at(&base, "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(error, ERR_DOC);
    }

    #[tokio::test]
    async fn skill_check_rejects_missing_or_unparsable_version() {
        let (base, requests, server) = serve(ok_response("---\nname: binance-web3\n---\n"));
        let error = skill_check_at(&base, "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(error, ERR_DOC);

        let (base, requests, server) =
            serve(ok_response("---\nversion: \"1.3.0\" trailing\n---\n"));
        let error = skill_check_at(&base, "binance-web3", "1.0.0")
            .await
            .unwrap_err();
        let _ = requests.recv();
        let _ = server.join();
        assert_eq!(error, ERR_DOC);
    }

    #[tokio::test]
    async fn skill_check_rejects_unsafe_skill_name_without_network() {
        for name in ["../evil", "a/b", "", ".hidden", "with space"] {
            assert_eq!(
                skill_check_at("http://127.0.0.1:1", name, "1.0.0")
                    .await
                    .unwrap_err(),
                ERR_SKILL_NAME,
                "{name}"
            );
        }
    }

    #[test]
    fn skill_version_parser_matches_bundle_regex() {
        assert_eq!(parse_skill_version("version: 1.2.3"), Some("1.2.3".into()));
        assert_eq!(
            parse_skill_version("  version:   '1.2.3'  "),
            Some("1.2.3".into())
        );
        assert_eq!(
            parse_skill_version("---\nname: x\nversion: \"2.0.0-beta.1\"\n---\n"),
            Some("2.0.0-beta.1".into())
        );
        assert_eq!(parse_skill_version("description: no version here"), None);
        assert_eq!(parse_skill_version("---\nname: x\n---\n"), None);
    }
}
