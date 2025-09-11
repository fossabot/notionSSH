Place certificate SHA256 fingerprints for api.notion.com here.

Format:
- One hex SHA256 per line (64 hex chars), case-insensitive. Colons are optional.
- Example:
  12:34:56:78:9A:BC:DE:F0:...:AB:CD:EF:01:23:45:67:89:0A:BC:DE:F0:12:34:56:78:9A:BC:DE:F0:12:34

Leaf certificate pin (default, always enforced)
How to get a leaf fingerprint (OpenSSL example):
  echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com 2>/dev/null \
    | openssl x509 -outform DER \
    | openssl dgst -sha256 -binary \
    | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'

CA certificate pin (optional, enabled when you answer Y)
- Save the SHA256 of the CA certificate DER in a file named like `ca_pins.sha256` or `something.ca.sha256`.
- To extract the intermediate CA certificate from the chain:
  echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com -showcerts \
    | awk '/BEGIN CERTIFICATE/{i++} i==2, /END CERTIFICATE/' \
    | openssl x509 -outform DER \
    | openssl dgst -sha256 -binary \
    | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'

Auto-managed CA pin
- When DoH or leaf pin verification fails, the app will ask to update the CA pin.
- If you answer Y, it writes the newly observed CA public key hash (SPKI SHA-256) and DER SHA-256 into `.notionSSH/ca.json`.
- If you answer N, the program exits.

Tooling
- Use `python scripts/update_notion_verify.py` to auto-populate `verify/notion-api.verify`.
- WARNING: The script OVERWRITES `verify/notion-api.verify`. It prompts for confirmation unless `--yes` is used.
- Optional args: `--host`, `--port`, `--output`, `--yes`.
- If OpenSSL is not available, the script records the leaf certificate and will attempt to fetch intermediate CA certificates via AIA from the leaf certificate (requires `cryptography`).

Security note:
- Certificate pinning reduces MITM risk but requires maintenance when Notion rotates certificates.
- If verification fails at startup, the program will exit with a warning.
