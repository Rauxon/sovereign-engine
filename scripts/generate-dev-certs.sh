#!/usr/bin/env bash
#
# Generate self-signed TLS certificates for local development.
# Outputs cert.pem and key.pem to the config/ directory.
#
# Usage: ./scripts/generate-dev-certs.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG_DIR="$PROJECT_ROOT/config"

CERT_FILE="$CONFIG_DIR/cert.pem"
KEY_FILE="$CONFIG_DIR/key.pem"
DAYS_VALID=365

# Create config directory if it doesn't exist
mkdir -p "$CONFIG_DIR"

# Check if certs already exist
if [ -f "$CERT_FILE" ] && [ -f "$KEY_FILE" ]; then
    echo "Certificates already exist at:"
    echo "  $CERT_FILE"
    echo "  $KEY_FILE"
    read -rp "Overwrite? [y/N] " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
fi

echo "Generating self-signed TLS certificate..."

openssl req -x509 -newkey rsa:4096 -nodes \
    -keyout "$KEY_FILE" \
    -out "$CERT_FILE" \
    -days "$DAYS_VALID" \
    -subj "/CN=localhost/O=SovereignEngine Dev" \
    -addext "subjectAltName=DNS:localhost,DNS:*.localhost,DNS:sovereign.local,IP:127.0.0.1,IP:::1"

echo ""
echo "Certificates generated:"
echo "  Certificate: $CERT_FILE"
echo "  Private key: $KEY_FILE"
echo "  Valid for:   $DAYS_VALID days"
echo ""
echo "To use TLS in docker-compose, add these environment variables:"
echo "  TLS_CERT_PATH=/config/cert.pem"
echo "  TLS_KEY_PATH=/config/key.pem"
echo "  LISTEN_ADDR=0.0.0.0:443"
