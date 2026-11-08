//! Remote access from the command line: which server to talk to, pairing, Tailscale.

use std::process::Command;

use anyhow::{Context, bail};
use composer_server::devices::Devices;
use composer_server::{WorkspaceHome, local_token};
use serde_json::{Value, json};

/// Where `login` stores a remote server: `<composer home>/remote.json`.
fn remote_file(home: &WorkspaceHome) -> std::path::PathBuf {
    home.home_root().join("remote.json")
}

/// The server this machine talks to: the one from `composer login`, else the local one.
pub fn client(home: &WorkspaceHome) -> anyhow::Result<(String, String)> {
    // The client crate's way (what the desktop does too); a standalone home or a token not made
    // yet falls through to the old one, which makes it.
    if !home.is_standalone()
        && let Ok(endpoint) =
            composer_client::Endpoint::discover(home.home_root(), home.name().as_str())
    {
        return Ok((endpoint.base().to_string(), endpoint.token().to_string()));
    }
    if let Ok(text) = std::fs::read_to_string(remote_file(home)) {
        let remote: Value = serde_json::from_str(&text).context("reading remote.json")?;
        if let (Some(server), Some(token)) = (remote["server"].as_str(), remote["token"].as_str()) {
            return Ok((server.to_string(), token.to_string()));
        }
    }
    let config = home.load_config()?;
    Ok((
        format!("http://{}{}", config.server.bind, home.route_base()),
        local_token(home)?,
    ))
}

/// `composer pair`: a one-time code, as a QR code for phones and a command for computers.
pub fn pair(home: &WorkspaceHome, name: &str) -> anyhow::Result<()> {
    let config = home.load_config()?;
    let code = Devices::new(&home.state()).create_code(name)?;
    // The pairing routes are the workspace's own: a device paired here opens only this workspace.
    let (base, served) = match tailscale_name() {
        Some(host) => (
            format!("https://{host}{}", home.route_base()),
            tailscale_serves(&config.server.bind),
        ),
        None => (
            format!("http://{}{}", config.server.bind, home.route_base()),
            true,
        ),
    };
    let url = format!("{base}/pair?code={code}");
    let qr = qrcode::QrCode::new(url.as_bytes())?
        .render::<qrcode::render::unicode::Dense1x2>()
        .dark_color(qrcode::render::unicode::Dense1x2::Light)
        .light_color(qrcode::render::unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!("{qr}");
    println!("workspace: {}  ({base})", home.name());
    println!("phone:     scan, or open {url}");
    println!("computer:  composer login {base} {code}");
    println!(
        "code {code} works once, for 10 minutes, and pairs a device named `{name}` with workspace `{}`.",
        home.name()
    );
    if !served {
        println!();
        println!("Tailscale is not serving composer yet. Expose it to your tailnet (only) with:");
        println!("  tailscale serve --bg http://{}", config.server.bind);
    }
    Ok(())
}

/// `composer devices [revoke <name>]`
pub fn devices(home: &WorkspaceHome, revoke: Option<String>) -> anyhow::Result<()> {
    let devices = Devices::new(&home.state());
    if let Some(name) = revoke {
        if !devices.revoke(&name)? {
            bail!("no device named `{name}`");
        }
        println!("revoked `{name}`: its token no longer works");
        return Ok(());
    }
    let list = devices.devices();
    if list.is_empty() {
        println!("no paired devices (pair one with `composer pair --name <name>`)");
    }
    for device in list {
        println!(
            "{:<16} paired {}",
            device.name,
            device
                .paired_at
                .get(..16)
                .unwrap_or(&device.paired_at)
                .replace('T', " ")
        );
    }
    Ok(())
}

/// `composer login <server> <code>`: pairs this machine with a remote server.
pub async fn login(
    home: &WorkspaceHome,
    server: &str,
    code: &str,
    name: Option<String>,
) -> anyhow::Result<()> {
    let server = server.trim_end_matches('/');
    let name = name.unwrap_or_else(hostname);
    let response = reqwest::Client::new()
        .post(format!("{server}/api/pair"))
        .json(&json!({ "code": code, "name": name }))
        .send()
        .await
        .with_context(|| format!("reaching {server}"))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_default();
    if !status.is_success() {
        bail!(
            "pairing failed: {}",
            body["error"].as_str().unwrap_or(status.as_str())
        );
    }
    std::fs::create_dir_all(home.home_root())?;
    let remote = json!({ "server": server, "token": body["token"], "name": body["name"] });
    std::fs::write(remote_file(home), serde_json::to_string_pretty(&remote)?)?;
    println!(
        "paired as `{}` with {server}; `composer` now connects there (`composer logout` to undo)",
        body["name"].as_str().unwrap_or(&name)
    );
    Ok(())
}

pub fn logout(home: &WorkspaceHome) -> anyhow::Result<()> {
    match std::fs::remove_file(remote_file(home)) {
        Ok(()) => println!("forgot the remote server; `composer` uses the local one again"),
        Err(_) => println!("not logged in to a remote server"),
    }
    Ok(())
}

/// This machine's MagicDNS name, e.g. `my-pc.tail1234.ts.net`.
#[allow(
    clippy::disallowed_methods,
    reason = "tailscale, run by the user's own remote command"
)]
fn tailscale_name() -> Option<String> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()?;
    let status: Value = serde_json::from_slice(&output.stdout).ok()?;
    let name = status["Self"]["DNSName"]
        .as_str()?
        .trim_end_matches('.')
        .to_string();
    (!name.is_empty()).then_some(name)
}

/// Whether `tailscale serve` already proxies to our bind address.
#[allow(
    clippy::disallowed_methods,
    reason = "tailscale, run by the user's own remote command"
)]
fn tailscale_serves(bind: &str) -> bool {
    Command::new("tailscale")
        .args(["serve", "status", "--json"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(bind))
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| "computer".into())
}
