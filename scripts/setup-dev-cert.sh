#!/usr/bin/env bash
set -euo pipefail

CERT_NAME="corvo-dev"

if security find-certificate -c "$CERT_NAME" >/dev/null 2>&1; then
    echo "Certificate '$CERT_NAME' already exists in Keychain."
    exit 0
fi

echo "Creating self-signed code signing certificate: $CERT_NAME..."
CONFIG_FILE=$(mktemp /tmp/corvo-dev-cert.cnf.XXXXXX)
KEY_FILE=$(mktemp /tmp/corvo-dev-key.pem.XXXXXX)
CERT_FILE=$(mktemp /tmp/corvo-dev-cert.crt.XXXXXX)
P12_FILE=$(mktemp /tmp/corvo-dev-pkg.p12.XXXXXX)

cat <<EOF > "$CONFIG_FILE"
[ req ]
default_bits        = 2048
distinguished_name  = req_distinguished_name
prompt              = no
x509_extensions     = v3_ca

[ req_distinguished_name ]
CN                  = $CERT_NAME

[ v3_ca ]
keyUsage            = critical, digitalSignature
extendedKeyUsage    = critical, codeSigning
EOF

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$KEY_FILE" -out "$CERT_FILE" \
    -config "$CONFIG_FILE"

/usr/bin/openssl pkcs12 -export -inkey "$KEY_FILE" -in "$CERT_FILE" \
    -out "$P12_FILE" -passout pass:corvodev

security import "$P12_FILE" -k ~/Library/Keychains/login.keychain-db -f pkcs12 -P corvodev -T /usr/bin/codesign

rm -f "$CONFIG_FILE" "$KEY_FILE" "$CERT_FILE" "$P12_FILE"

echo "Certificate '$CERT_NAME' created and imported successfully."
