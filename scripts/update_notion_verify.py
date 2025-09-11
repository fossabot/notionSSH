#!/usr/bin/env python3
"""
Auto-populate verify/notion-api.verify with current certificate hashes for api.notion.com.

WARNING: This script OVERWRITES verify/notion-api.verify.

It attempts to collect the full certificate chain using `openssl s_client -showcerts` if
OpenSSL is available. Otherwise, it falls back to Python's ssl module and always records
the leaf certificate. If the 'cryptography' package is available, the script will also
try to fetch intermediate CA certificates via AIA (Authority Information Access) URLs
from the leaf certificate and include their DER and SPKI SHA-256 hashes.
"""
import argparse
import base64
import hashlib
import json
import re
import shutil
import socket
import ssl
import sys
from pathlib import Path
from urllib.request import urlopen, Request


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest().upper()


def pem_blocks_from_text(text: str):
    pattern = re.compile(r"-----BEGIN CERTIFICATE-----\s*(.*?)\s*-----END CERTIFICATE-----",
                         re.DOTALL)
    for m in pattern.finditer(text):
        b64 = m.group(1).replace("\n", "").replace("\r", "")
        try:
            der = base64.b64decode(b64)
            yield der
        except Exception:
            continue


def get_chain_with_openssl(host: str, port: int) -> list[bytes]:
    exe = shutil.which("openssl")
    if not exe:
        return []
    import subprocess
    cmd = [
        exe, "s_client", "-connect", f"{host}:{port}", "-servername", host, "-showcerts"
    ]
    try:
        out = subprocess.check_output(cmd, stderr=subprocess.STDOUT, text=True, timeout=20)
    except Exception:
        return []
    return list(pem_blocks_from_text(out))


def get_leaf_with_ssl(host: str, port: int) -> bytes | None:
    ctx = ssl.create_default_context()
    with socket.create_connection((host, port), timeout=10) as sock:
        with ctx.wrap_socket(sock, server_hostname=host) as ssock:
            der = ssock.getpeercert(binary_form=True)
            return der


def spki_sha256_hex(cert_der: bytes) -> str | None:
    try:
        from cryptography import x509
        from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
        cert = x509.load_der_x509_certificate(cert_der)
        spki = cert.public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo)
        return sha256_hex(spki)
    except Exception:
        return None


def try_fetch_intermediate_cas_via_aia(leaf_der: bytes) -> list[bytes]:
    try:
        from cryptography import x509
        from cryptography.x509.oid import ExtensionOID
    except Exception:
        return []
    try:
        cert = x509.load_der_x509_certificate(leaf_der)
        try:
            aia = cert.extensions.get_extension_for_oid(ExtensionOID.AUTHORITY_INFORMATION_ACCESS).value
        except Exception:
            return []
        uris = []
        for desc in aia:
            try:
                # 1.3.6.1.5.5.7.48.2 = caIssuers
                if getattr(desc.access_method, "dotted_string", "") == "1.3.6.1.5.5.7.48.2":
                    uris.append(str(desc.access_location.value))
            except Exception:
                continue
        ders: list[bytes] = []
        for u in uris:
            der = fetch_der_from_url(u)
            if der:
                ders.append(der)
        return ders
    except Exception:
        return []


def fetch_der_from_url(url: str) -> bytes | None:
    try:
        req = Request(url, headers={"User-Agent": "notionSSH-verify/1.0"})
        with urlopen(req, timeout=15) as resp:
            data = resp.read()
    except Exception:
        return None
    if b"BEGIN CERTIFICATE" in data:
        text = data.decode("ascii", errors="ignore")
        blocks = list(pem_blocks_from_text(text))
        return blocks[0] if blocks else None
    return data


def main():
    parser = argparse.ArgumentParser(description="Update verify/notion-api.verify with current hashes")
    parser.add_argument("--host", default="api.notion.com", help="Target host (default: api.notion.com)")
    parser.add_argument("--port", type=int, default=443, help="Target port (default: 443)")
    parser.add_argument("--yes", action="store_true", help="Overwrite without confirmation")
    parser.add_argument("--output", default=None, help="Output file path (default: verify/notion-api.verify)")
    args = parser.parse_args()

    script_dir = Path(__file__).resolve().parent
    repo_root = script_dir.parent
    default_out = repo_root / "verify" / "notion-api.verify"
    out_path = Path(args.output) if args.output else default_out
    out_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"[!] WARNING: This will overwrite {out_path}")
    if not args.yes:
        ans = input("Proceed? [Y/N] ").strip().lower()
        if ans not in ("y", "yes"):
            print("Aborted.")
            return 1

    chain = get_chain_with_openssl(args.host, args.port)
    used_openssl = bool(chain)
    if not chain:
        leaf = get_leaf_with_ssl(args.host, args.port)
        if not leaf:
            print("Failed to obtain certificate(s).")
            return 2
        chain = [leaf]
        # Try AIA fetch for intermediates when not using OpenSSL
        chain.extend(try_fetch_intermediate_cas_via_aia(leaf))

    leaf_der = chain[0]
    leaf_sha = sha256_hex(leaf_der)
    ca_ders = chain[1:]

    ca_der_hashes = [sha256_hex(der) for der in ca_ders]
    ca_spki_hashes = []
    for der in ca_ders:
        h = spki_sha256_hex(der)
        if h:
            ca_spki_hashes.append(h)

    data = {
        "leaf_sha256": [leaf_sha],
        "ca_der_sha256": ca_der_hashes,
        "ca_spki_sha256": ca_spki_hashes,
        "meta": {
            "host": args.host,
            "port": args.port,
            "source": "openssl" if used_openssl else "ssl+aia",
        },
    }

    out_path.write_text(json.dumps(data, indent=2))
    print(f"[*] Wrote {out_path}")
    print(f" - leaf_sha256: {leaf_sha}")
    if ca_der_hashes:
        print(f" - ca_der_sha256: {len(ca_der_hashes)} entries")
    if ca_spki_hashes:
        print(f" - ca_spki_sha256: {len(ca_spki_hashes)} entries")
    if not used_openssl and len(chain) <= 1:
        print("[!] Note: OpenSSL not found and no AIA intermediates fetched; only the leaf certificate was recorded.")
        print("    Install OpenSSL for full chain collection or ensure AIA is reachable.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

