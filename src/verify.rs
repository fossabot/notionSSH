use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;

// Domain we verify for Notion API
const NOTION_HOST: &str = "api.notion.com";
const NOTION_PORT: u16 = 443;

pub fn verify_notion_endpoint(enable_ca_pubkey: bool) -> Result<()> {
    // If CA verification is requested but no saved CA pins exist, try to
    // auto-generate .notionSSH/ca.json from ./verify contents before verifying.
    if enable_ca_pubkey && !saved_ca_pins_exist() {
        match write_ca_json_from_verify(Path::new("./verify"), Path::new(".notionSSH/ca.json")) {
            Ok(true) => println!("[*] Prepared .notionSSH/ca.json from ./verify"),
            Ok(false) => { /* no CA pins in ./verify; skip silently */ }
            Err(_e) => { /* ignore preparation errors to avoid noisy warnings */ }
        }
    }
    // 1) Resolve via DoH and compare with system resolver. Do not fail hard here;
    //    record mismatch to prompt user later.
    let mut doh_ok = true;
    match doh_ips_union(NOTION_HOST) {
        Ok(doh_ips) => {
            if doh_ips.is_empty() { doh_ok = false; }
            match system_resolve(NOTION_HOST, NOTION_PORT) {
                Ok(system_ips) => {
                    if system_ips.is_empty() { doh_ok = false; }
                    let overlap: HashSet<IpAddr> = system_ips.intersection(&doh_ips).cloned().collect();
                    if overlap.is_empty() { doh_ok = false; }
                }
                Err(_) => { doh_ok = false; }
            }
        }
        Err(_) => { doh_ok = false; }
    }

    // 2) TLS connect with rustls using CA roots (chain verification) and collect peer certs.
    tls_verify_and_pin(NOTION_HOST, NOTION_PORT, enable_ca_pubkey, doh_ok)
}

fn doh_ips_union(host: &str) -> Result<HashSet<IpAddr>> {
    let client = Client::builder()
        .user_agent("notionSSH-verify/1.0")
        .build()?;

    let mut ips: HashSet<IpAddr> = HashSet::new();

    // Cloudflare DoH (A, AAAA)
    ips.extend(fetch_doh_ips(&client, format!("https://cloudflare-dns.com/dns-query?name={host}&type=A"))?);
    ips.extend(fetch_doh_ips(&client, format!("https://cloudflare-dns.com/dns-query?name={host}&type=AAAA"))?);

    // Google DoH (A, AAAA)
    ips.extend(fetch_doh_ips(&client, format!("https://dns.google/resolve?name={host}&type=A"))?);
    ips.extend(fetch_doh_ips(&client, format!("https://dns.google/resolve?name={host}&type=AAAA"))?);

    Ok(ips)
}

fn fetch_doh_ips(client: &Client, url: String) -> Result<HashSet<IpAddr>> {
    let mut out = HashSet::new();
    let resp = client
        .get(&url)
        .header("accept", "application/dns-json")
        .send()
        .with_context(|| format!("DoH request failed: {}", url))?;
    let v: Value = resp.json().with_context(|| format!("Invalid DoH JSON from {}", url))?;
    if let Some(ans) = v.get("Answer").and_then(|v| v.as_array()) {
        for a in ans {
            let ty = a.get("type").and_then(|v| v.as_u64()).unwrap_or(0);
            if ty == 1 || ty == 28 {
                if let Some(data) = a.get("data").and_then(|v| v.as_str()) {
                    if let Ok(ip) = data.parse::<IpAddr>() { out.insert(ip); }
                }
            }
        }
    }
    Ok(out)
}

fn system_resolve(host: &str, port: u16) -> Result<HashSet<IpAddr>> {
    let addrs = (host, port).to_socket_addrs()?;
    Ok(addrs.map(|sa| sa.ip()).collect())
}

fn tls_verify_and_pin(host: &str, port: u16, enable_ca_pubkey: bool, doh_ok: bool) -> Result<()> {
    use rustls::{ClientConfig, ClientConnection, OwnedTrustAnchor, RootCertStore, ServerName, StreamOwned};
    use webpki_roots::TLS_SERVER_ROOTS;

    // Build rustls config with CA roots -> this performs chain verification during handshake.
    let mut roots = RootCertStore::empty();
    roots.add_server_trust_anchors(TLS_SERVER_ROOTS.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    let config = Arc::new(
        ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );

    let server_name = ServerName::try_from(host).map_err(|_| anyhow!("Invalid SNI host"))?;

    // Choose an address via the system resolver; chain validation will still apply.
    let addr = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow!("No address for {}", host))?;
    let tcp = TcpStream::connect(addr)?;
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;

    let conn = ClientConnection::new(Arc::clone(&config), server_name)?;
    let mut tls = StreamOwned::new(conn, tcp);

    // Trigger handshake by sending a minimal request; then read a bit.
    let req = format!(
        "GET /v1/users/me HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        host
    );
    tls.write_all(req.as_bytes())?;
    let mut buf = [0u8; 512];
    let _ = tls.read(&mut buf).ok();

    // Obtain peer certs from the finished connection (chain already verified by rustls).
    let certs = tls.conn.peer_certificates().ok_or_else(|| anyhow!("No peer certificates"))?;

    // Load expected leaf pins from ./verify directory.
    let expected = load_expected_pins(Path::new("./verify"))?;

    // Compute SHA256 of DER for each certificate in the chain and check if any match the expected set.
    let mut any_match = false;
    for c in certs {
        let mut hasher = Sha256::new();
        hasher.update(&c.0);
        let digest = hasher.finalize();
        let hex = hex_upper(&digest);
        if expected.contains(&hex) {
            any_match = true;
            break;
        }
    }

    // If leaf pinning fails, offer CA update prompt after optionally checking CA pins.

    // Optional: CA certificate fingerprint pinning (checks non-leaf certificates in the chain)
    if enable_ca_pubkey {
        let expected_ca = load_expected_ca_pins(Path::new("./verify"))?
            .union(&load_ca_json_pins(Path::new(".notionSSH/ca.json"))?)
            .cloned()
            .collect::<HashSet<String>>();
        if !expected_ca.is_empty() {
            let mut ca_match = false;
            for (idx, c) in certs.iter().enumerate() {
                if idx == 0 { continue; } // skip leaf
                // Accept either DER SHA256 or SPKI SHA256 pins
                let der_hex = {
                    let mut h = Sha256::new();
                    h.update(&c.0);
                    hex_upper(&h.finalize())
                };
                if expected_ca.contains(&der_hex) {
                    ca_match = true; break;
                }
                if let Ok(spki_hex) = spki_sha256_hex(&c.0) {
                    if expected_ca.contains(&spki_hex) { ca_match = true; break; }
                }
            }
            if !ca_match {
                return Err(anyhow!("CA certificate pin mismatch for {}", host));
            }
        }
    }
    if any_match && doh_ok {
        return Ok(());
    }

    // DoH mismatch or leaf pin mismatch: ask user whether to update CA.
    // We also persist newly observed CA public key to .notionSSH/ca.json if user agrees.
    if !doh_ok {
        eprintln!("[!] DoH DNS verification did not match system DNS for {}", host);
    }
    if !any_match {
        eprintln!("[!] Leaf certificate pin mismatch for {}", host);
    }
    prompt_and_maybe_update_ca(&tls.conn)
}

fn load_expected_pins(dir: &Path) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    // Prefer notion-api.verify format if present
    let verify_file = dir.join("notion-api.verify");
    if verify_file.exists() {
        let (leaf, _ca_der, _ca_spki) = load_verify_format(&verify_file)?;
        set.extend(leaf);
    }
    if !dir.exists() {
        return Ok(set);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() { continue; }
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()) == Some("notion-api.verify") { continue; }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if !matches!(ext, "txt" | "pin" | "sha256") { continue; }
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        for line in content.lines() {
            let s = line.trim().replace(':', "");
            if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                set.insert(s.to_uppercase());
            }
        }
    }
    Ok(set)
}

fn load_expected_ca_pins(dir: &Path) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    // Prefer notion-api.verify format if present
    let verify_file = dir.join("notion-api.verify");
    if verify_file.exists() {
        let (_leaf, ca_der, ca_spki) = load_verify_format(&verify_file)?;
        set.extend(ca_der);
        set.extend(ca_spki);
    }
    if !dir.exists() { return Ok(set); }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() { continue; }
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        if name == "notion-api.verify" { continue; }
        // Heuristic: files named like ca_*.sha256 or *.ca.sha256 contain CA pins
        if !(name.starts_with("ca_") || name.contains(".ca.")) { continue; }
        let content = fs::read_to_string(&path).unwrap_or_default();
        for line in content.lines() {
            let s = line.trim().replace(':', "");
            if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                set.insert(s.to_uppercase());
            }
        }
    }
    Ok(set)
}

fn load_ca_json_pins(path: &Path) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    if !path.exists() { return Ok(set); }
    let s = fs::read_to_string(path)?;
    let v: Value = serde_json::from_str(&s)?;
    if let Some(hex) = v.get("spki_sha256").and_then(|v| v.as_str()) {
        set.insert(hex.trim().to_uppercase());
    }
    if let Some(hex) = v.get("der_sha256").and_then(|v| v.as_str()) {
        set.insert(hex.trim().to_uppercase());
    }
    Ok(set)
}

// Public helper: check if default saved CA pins exist in .notionSSH/ca.json
pub fn saved_ca_pins_exist() -> bool {
    load_ca_json_pins(Path::new(".notionSSH/ca.json")).map(|s| !s.is_empty()).unwrap_or(false)
}

fn write_ca_json_from_verify(verify_dir: &Path, out_json: &Path) -> Result<bool> {
    use chrono::Utc;
    use std::fs::{create_dir_all, File};

    let verify_file = verify_dir.join("notion-api.verify");
    let mut ca_der_list: Vec<String> = Vec::new();
    let mut ca_spki_list: Vec<String> = Vec::new();

    if verify_file.exists() {
        let (_leaf, ca_der, ca_spki) = load_verify_format(&verify_file)?;
        ca_der_list.extend(ca_der.into_iter());
        ca_spki_list.extend(ca_spki.into_iter());
    }
    // Fallback: scan directory for CA pin files if verify file missing
    if ca_der_list.is_empty() && ca_spki_list.is_empty() && verify_dir.exists() {
        for entry in fs::read_dir(verify_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() { continue; }
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            if name == "notion-api.verify" { continue; }
            if !(name.starts_with("ca_") || name.contains(".ca.")) { continue; }
            let content = fs::read_to_string(&path).unwrap_or_default();
            for line in content.lines() {
                let s = line.trim().replace(':', "").to_uppercase();
                if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                    // Without format hints, store as DER list by default
                    ca_der_list.push(s);
                }
            }
        }
    }

    if ca_der_list.is_empty() && ca_spki_list.is_empty() {
        return Ok(false);
    }

    let dir = out_json.parent().unwrap_or_else(|| Path::new("."));
    if !dir.exists() { create_dir_all(dir)?; }
    let mut obj = serde_json::Map::new();
    obj.insert("note".into(), Value::String("Stored CA public key for api.notion.com (from ./verify)".into()));
    obj.insert("host".into(), Value::String(NOTION_HOST.into()));
    if let Some(spki) = ca_spki_list.first() { obj.insert("spki_sha256".into(), Value::String(spki.clone())); }
    if let Some(der) = ca_der_list.first() { obj.insert("der_sha256".into(), Value::String(der.clone())); }
    obj.insert("stored_at".into(), Value::String(Utc::now().to_rfc3339()));
    let json = Value::Object(obj);
    let mut f = File::create(out_json)?;
    f.write_all(serde_json::to_string_pretty(&json)?.as_bytes())?;
    Ok(true)
}

fn prompt_and_maybe_update_ca(conn: &rustls::ClientConnection) -> Result<()> {
    use chrono::Utc;
    use std::fs::{create_dir_all, File};

    let certs = conn
        .peer_certificates()
        .ok_or_else(|| anyhow!("No peer certificates"))?;
    if certs.len() < 2 {
        return Err(anyhow!("Cannot update CA: no CA certificate found in peer chain"));
    }
    // Choose first non-leaf as CA candidate
    let ca_der = &certs[1].0;
    let der_hex = {
        let mut h = Sha256::new();
        h.update(ca_der);
        hex_upper(&h.finalize())
    };
    let spki_hex = spki_sha256_hex(ca_der).unwrap_or_else(|_| String::new());

    eprintln!("[?] Do you want to update CA pin with the newly observed certificate? [Y/N]");
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
    let yes = matches!(input.trim().to_uppercase().as_str(), "Y" | "YES");
    if !yes {
        return Err(anyhow!("User declined CA update"));
    }

    let dir = Path::new(".notionSSH");
    if !dir.exists() { create_dir_all(dir)?; }
    let path = dir.join("ca.json");
    let now = Utc::now().to_rfc3339();
    let mut obj = serde_json::Map::new();
    obj.insert("note".into(), Value::String("Stored CA public key for api.notion.com".into()));
    obj.insert("host".into(), Value::String(NOTION_HOST.into()));
    if !spki_hex.is_empty() { obj.insert("spki_sha256".into(), Value::String(spki_hex)); }
    obj.insert("der_sha256".into(), Value::String(der_hex));
    obj.insert("stored_at".into(), Value::String(now));
    let json = Value::Object(obj);
    let mut f = File::create(&path)?;
    f.write_all(serde_json::to_string_pretty(&json)?.as_bytes())?;
    println!("[*] CA pin updated at {}", path.display());
    Ok(())
}

fn spki_sha256_hex(cert_der: &[u8]) -> Result<String> {
    use x509_parser::prelude::*;
    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|_| anyhow!("Failed to parse certificate DER"))?;
    let spki = cert.tbs_certificate.subject_pki.subject_public_key.data;
    let mut h = Sha256::new();
    h.update(spki);
    Ok(hex_upper(&h.finalize()))
}

// notion-api.verify format loader
// JSON object with fields:
// {
//   "leaf_sha256": ["HEX..."],
//   "ca_der_sha256": ["HEX..."],
//   "ca_spki_sha256": ["HEX..."]
// }
fn load_verify_format(path: &Path) -> Result<(HashSet<String>, HashSet<String>, HashSet<String>)> {
    let s = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let v: Value = serde_json::from_str(&s)
        .with_context(|| format!("Invalid JSON in {}", path.display()))?;
    let mut leaf = HashSet::new();
    let mut ca_der = HashSet::new();
    let mut ca_spki = HashSet::new();
    if let Some(arr) = v.get("leaf_sha256").and_then(|x| x.as_array()) {
        for it in arr {
            if let Some(s) = it.as_str() {
                let h = s.trim().replace(':', "").to_uppercase();
                if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) { leaf.insert(h); }
            }
        }
    }
    if let Some(arr) = v.get("ca_der_sha256").and_then(|x| x.as_array()) {
        for it in arr {
            if let Some(s) = it.as_str() {
                let h = s.trim().replace(':', "").to_uppercase();
                if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) { ca_der.insert(h); }
            }
        }
    }
    if let Some(arr) = v.get("ca_spki_sha256").and_then(|x| x.as_array()) {
        for it in arr {
            if let Some(s) = it.as_str() {
                let h = s.trim().replace(':', "").to_uppercase();
                if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) { ca_spki.insert(h); }
            }
        }
    }
    Ok((leaf, ca_der, ca_spki))
}

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_format() {
        assert_eq!(hex_upper(&[0xAB, 0xCD, 0x00, 0x12]), "ABCD0012");
    }
}
