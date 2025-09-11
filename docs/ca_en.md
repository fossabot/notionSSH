# CA Certificate Verification

## Language / 언어
- [🇰🇷 한국어](ca.md)
- [🇺🇸 English](ca_en.md)

---

NotionSSH provides multi-layer security verification to prevent man-in-the-middle (MITM) attacks. This feature strengthens security through certificate chain verification, DoH DNS verification, and certificate pinning when communicating with Notion API servers.

## Security Verification Steps

NotionSSH performs 3-step verification:

1. **CA Certificate Chain Verification (1/3)**: Certificate chain verification through standard CAs
2. **DoH DNS Verification (2/3)**: IP address verification through DNS over HTTPS
3. **Certificate Pin Verification (3/3)**: Comparison with pre-defined certificate fingerprints

## Configuration Methods

### Automatic Configuration (Recommended)

When you run NotionSSH for the first time, you'll see a message asking whether to use CA certificate verification:

```
Use CA public key fingerprint verification? [Y/N]
```

- **Enter Y**: Enable CA certificate verification
- **Enter N**: Use only basic TLS verification

### Manual Configuration

#### 1. Configure verify Directory

Store certificate fingerprints in the `verify/` directory at the project root.

**File Structure:**
```
verify/
├── notion-api.verify        # JSON format (recommended)
├── README.txt              # Configuration guide
└── notion-api.verify.example # Example file
```

#### 2. notion-api.verify File Format

```json
{
  "leaf_sha256": [
    "LEAF_CERTIFICATE_SHA256_FINGERPRINT_HERE"
  ],
  "ca_der_sha256": [
    "CA_CERTIFICATE_DER_SHA256_HERE"
  ],
  "ca_spki_sha256": [
    "CA_PUBLIC_KEY_SPKI_SHA256_HERE"
  ]
}
```

#### 3. .notionSSH/ca.json File

CA pin repository automatically generated at runtime:

```json
{
  "note": "Stored CA public key for api.notion.com",
  "host": "api.notion.com",
  "spki_sha256": "CA_PUBLIC_KEY_SPKI_SHA256",
  "der_sha256": "CA_CERTIFICATE_DER_SHA256",
  "stored_at": "2025-09-11T10:30:45Z"
}
```

## How to Obtain Certificate Fingerprints

### Automatic Acquisition (Using Scripts)

Use the Python script included in the project:

```bash
python scripts/update_notion_verify.py --yes
```

**Script Options:**
- `--host`: Target host (default: api.notion.com)
- `--port`: Port number (default: 443)
- `--output`: Output file (default: verify/notion-api.verify)
- `--yes`: Auto-run without confirmation

### Manual Acquisition (Using OpenSSL)

#### Get Leaf Certificate Fingerprint

```bash
echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com 2>/dev/null \
  | openssl x509 -outform DER \
  | openssl dgst -sha256 -binary \
  | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'
```

#### Get CA Certificate Fingerprint

```bash
echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com -showcerts \
  | awk '/BEGIN CERTIFICATE/{i++} i==2, /END CERTIFICATE/' \
  | openssl x509 -outform DER \
  | openssl dgst -sha256 -binary \
  | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'
```

## Detailed Verification Process

### Step 1: CA Certificate Chain Verification

- **Purpose**: Basic TLS verification through standard CAs
- **Method**: Chain verification using rustls and webpki_roots
- **Success**: `[*] (1/3) CA certificate chain: PASS`
- **Failure**: Connection terminates immediately

### Step 2: DoH DNS Verification

- **Purpose**: Prevent DNS spoofing attacks
- **Method**: 
  - DNS queries through Cloudflare DoH (1.1.1.1) and Google DoH (8.8.8.8)
  - Cross-verification with system DNS results
- **Success**: `[*] (2/3) DoH DNS verification: PASS`
- **Failure**: `[!] (2/3) DoH DNS verification: FAIL`

### Step 3: Certificate Pin Verification

- **Purpose**: Force verification of specific certificates/CAs
- **Method**:
  - Compare leaf certificate SHA256 fingerprints
  - Compare CA certificate DER/SPKI SHA256 fingerprints
- **Success**: `[*] (3/3) Certificate pinning: PASS`
- **Failure**: 
  - `[!] (3/3) Certificate pinning: FAIL (leaf mismatch)`
  - `[!] (3/3) Certificate pinning: FAIL (CA pin mismatch)`

## Auto-Update Features

### CA Pin Auto-Update

When certificate pin verification fails, you'll be asked whether to update with the new CA certificate:

```
[?] Do you want to update CA pin with the newly observed certificate? [Y/N]
```

- **Enter Y**: Save new CA pin to `.notionSSH/ca.json`
- **Enter N**: Exit program

### Leaf Certificate Update

When leaf certificate mismatch occurs, using the Python script is recommended:

```
[!] To refresh pins, run: python scripts/update_notion_verify.py --yes
```

## Security Recommendations

### Regular Pin Updates
- Pin updates required whenever Notion renews certificates
- Run automation scripts periodically to synchronize pins
- Prepare backup pins in advance to prevent service interruption

### Verification Mode Selection
```bash
# Maximum security (recommended for production)
# Select Y to enable all 3-step verification

# Basic security (development/test environment)
# Select N to use only CA chain verification
```

### Network Environment Considerations
- **Firewall Environment**: DoH DNS queries may be blocked
- **Proxy Environment**: Certificate pin verification may fail
- **Restricted Environment**: Consider using only basic TLS verification

## Configuration Examples

### Basic Configuration (New Installation)

1. First run of NotionSSH
2. Select CA verification activation (Y)
3. Automatically create `.notionSSH/ca.json`
4. Verify normal operation

### Manual Configuration (Advanced Users)

1. Prepare `verify/notion-api.verify` file:
```json
{
  "leaf_sha256": ["ACTUAL_LEAF_CERTIFICATE_FINGERPRINT"],
  "ca_der_sha256": ["ACTUAL_CA_DER_FINGERPRINT"],
  "ca_spki_sha256": ["ACTUAL_CA_SPKI_FINGERPRINT"]
}
```

2. Automatically create `.notionSSH/ca.json` when running NotionSSH
3. Verify all 3-step verification passes

### Update Scenario

**Failure due to certificate renewal:**
```bash
# 1. Obtain new pins with script
python scripts/update_notion_verify.py --yes

# 2. Restart NotionSSH
./notionSSH

# 3. Verify verification passes
[*] (1/3) CA certificate chain: PASS
[*] (2/3) DoH DNS verification: PASS
[*] (3/3) Certificate pinning: PASS
```

## Troubleshooting

### Common Errors

#### "Cannot proceed without CA verification. please check the network or CA configuration. (1/3)"
- **Cause**: Basic TLS connection failure
- **Solution**: Check network connection and firewall settings

#### "(2/3) DoH DNS verification: FAIL"
- **Cause**: DoH DNS query failure or IP mismatch
- **Solution**: 
  - Check internet connection
  - Check if DoH services are blocked
  - Verify proxy settings

#### "(3/3) Certificate pinning: FAIL (leaf mismatch)"
- **Cause**: Leaf certificate has changed
- **Solution**: Run `python scripts/update_notion_verify.py --yes`

#### "(3/3) Certificate pinning: FAIL (CA pin mismatch)"
- **Cause**: CA certificate has changed
- **Solution**: Select Y to auto-update CA pin

### Configuration File Errors

#### notion-api.verify File Corruption
```bash
# Restore from backup or regenerate
python scripts/update_notion_verify.py --yes
```

#### .notionSSH/ca.json File Corruption
```bash
# Delete file and re-run
rm .notionSSH/ca.json
./notionSSH
```

### Environment-Specific Configuration

#### Development Environment
- Disable CA verification (select N)
- Use only basic TLS verification
- Support rapid development cycle

#### Staging Environment
- Enable CA verification (select Y)
- Regular pin update script execution
- Same security settings as production

#### Production Environment
- Enable CA verification (select Y)
- Automated pin updates
- Monitoring and alerting setup
- Track certificate renewal schedule

## Script Tools

### update_notion_verify.py

**Basic Usage:**
```bash
python scripts/update_notion_verify.py
```

**Advanced Options:**
```bash
# Specify different host
python scripts/update_notion_verify.py --host custom.api.com --port 443

# Specify output file
python scripts/update_notion_verify.py --output custom-verify.json

# Auto-run without confirmation
python scripts/update_notion_verify.py --yes
```

**Dependencies:**
- Python 3.6+
- `cryptography` package (for CA certificate acquisition via AIA extension)
- Optional: OpenSSL (command-line tools)

## Security Considerations

### Advantages
- **MITM Attack Prevention**: Strong authentication through certificate pinning
- **DNS Spoofing Prevention**: Cross-verification of multiple DoH services
- **Auto-Update**: Minimize user intervention
- **Multi-Layer Security**: 3-step independent verification

### Precautions
- **Operational Complexity**: Pin updates required when certificates are renewed
- **Availability Risk**: Service interruption if pins are configured incorrectly
- **Network Dependency**: Depends on DoH service availability
- **Maintenance**: Regular pin update tasks required

### Best Practices
1. **Backup Pins**: Prepare multiple valid pins in advance
2. **Automation**: Regular updates through scripts
3. **Monitoring**: Set up alerts for verification failures
4. **Documentation**: Document certificate renewal procedures
5. **Testing**: Verify in staging environment first

This CA certificate verification system allows NotionSSH to maintain high-level security while providing user convenience.