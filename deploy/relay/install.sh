#!/usr/bin/env bash
# balchat-relay installer for Debian/Ubuntu VPS.
#
# Usage:
#   sudo ./install.sh
#
# Assumes the relay binary for THIS architecture sits next to this script as:
#   ./balchat-relay
# (the deploy bundle ships the right one for your VPS arch).

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "error: ejecutame con sudo" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$SCRIPT_DIR/balchat-relay"
UNIT="$SCRIPT_DIR/balchat-relay.service"

if [[ ! -x "$BIN" ]]; then
  echo "error: no encuentro el binario en $BIN" >&2
  exit 1
fi
if [[ ! -f "$UNIT" ]]; then
  echo "error: no encuentro $UNIT" >&2
  exit 1
fi

echo "[1/6] creando usuario sin shell 'balchat-relay'..."
if ! id -u balchat-relay >/dev/null 2>&1; then
  useradd --system --no-create-home --shell /usr/sbin/nologin balchat-relay
fi

echo "[2/6] preparando dirs (/opt/balchat-relay, /var/lib/balchat-relay)..."
install -d -m 0755 /opt/balchat-relay
install -d -m 0700 -o balchat-relay -g balchat-relay /var/lib/balchat-relay

echo "[3/6] instalando binario en /opt/balchat-relay/balchat-relay..."
install -m 0755 "$BIN" /opt/balchat-relay/balchat-relay

echo "[4/6] instalando unit systemd..."
install -m 0644 "$UNIT" /etc/systemd/system/balchat-relay.service
systemctl daemon-reload

echo "[5/6] habilitando + arrancando servicio..."
systemctl enable balchat-relay
systemctl restart balchat-relay

echo "[6/6] esperando a que el relay levante el onion service (puede tardar 1-3 min en primer arranque)..."
ONION=""
for i in $(seq 1 60); do
  ONION=$(journalctl -u balchat-relay --since "2 min ago" --no-pager 2>/dev/null \
    | grep -oE '[a-z2-7]{56}\.onion' | tail -1 || true)
  if [[ -n "$ONION" ]]; then break; fi
  sleep 3
done

echo
echo "================================================================"
if [[ -n "$ONION" ]]; then
  echo "  ✓ balchat-relay corriendo"
  echo "  Onion address: $ONION"
  echo
  echo "  Pega esto en la app balchat (Settings → Mi buzón) o pásalo"
  echo "  al desarrollador para hardcodearlo como buzón recomendado."
else
  echo "  ⚠ relay arrancado pero todavía no veo el onion en logs."
  echo "  Mira:  journalctl -u balchat-relay -f"
  echo "  Es normal si Tor está bootstrapeando (3-5 min en primera vez)."
fi
echo "================================================================"
echo
echo "Status:    systemctl status balchat-relay"
echo "Logs:      journalctl -u balchat-relay -f"
echo "Stop:      sudo systemctl stop balchat-relay"
echo "Uninstall: sudo systemctl disable --now balchat-relay && sudo rm -rf /opt/balchat-relay /var/lib/balchat-relay /etc/systemd/system/balchat-relay.service && sudo userdel balchat-relay"
