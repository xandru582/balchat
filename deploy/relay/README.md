# balchat-relay — deploy en VPS Hetzner

Onion service no-confiable que almacena blobs cifrados (MLS) cuando los peers están offline.
**No ve contenido**: solo blobs opacos. Ve únicamente metadata (qué cola, cuándo, cuánto bytes).

## Pre-requisitos

- VPS Debian 12 / Ubuntu 22.04+ (cualquier tamaño Hetzner — el relay consume <100 MB RAM y 0% CPU en idle).
- Acceso root via SSH.
- Salida a internet en puerto 9001+ (Tor entry guards). NO necesita IP pública ni puertos abiertos en el firewall: el onion service es inbound-via-Tor.

## Arquitecturas soportadas

| Hetzner instance | Architecture | Binary |
|---|---|---|
| CX11, CX21, CX31, CPX11, CPX21, … | x86_64 (Intel/AMD) | `balchat-relay-x86_64` |
| CAX11, CAX21, CAX31, … | aarch64 (ARM Ampere) | `balchat-relay-aarch64` |

## Pasos

1. **En tu Mac**, copia el bundle al servidor (sustituye IP, y la arch que toque):

   ```bash
   # Renombra el binario que toca a 'balchat-relay' antes de subir
   cp balchat-relay-x86_64 balchat-relay   # o el aarch64 si es CAX
   scp -r ./deploy/relay/ root@TU_IP_HETZNER:/root/relay-deploy/
   ```

2. **En la VPS** (SSH como root):

   ```bash
   cd /root/relay-deploy/
   chmod +x install.sh balchat-relay
   sudo ./install.sh
   ```

3. **Espera 1-3 minutos** en el primer arranque mientras Arti bootstrapea su circuito Tor y publica el descriptor del onion. El script lo detecta y te imprime:

   ```
   ✓ balchat-relay corriendo
   Onion address: abc123…xyz.onion
   ```

4. **Pásame ese onion** y lo hardcodeo como buzón recomendado en la app.

## Comprobaciones útiles

```bash
# Status:
systemctl status balchat-relay

# Logs en vivo (ves cada PUT/GET y arranque de Arti):
journalctl -u balchat-relay -f

# Ver onion address otra vez después del arranque:
journalctl -u balchat-relay --no-pager | grep -oE '[a-z2-7]{56}\.onion' | tail -1
```

## Hardening

El systemd unit ya viene con hardening estricto (sin nuevos privs, FS read-only excepto data-dir, sin namespaces, sin tmp privado). Lo único que el relay necesita escribir es `/var/lib/balchat-relay/`.

La keystore del onion (en `/var/lib/balchat-relay/`) **no se debe perder**: si la borras, el onion address cambia y todos los usuarios quedan con un buzón muerto. Haz backup periódico de `/var/lib/balchat-relay/`.

## Costes Hetzner

El más barato suficiente: **CX22** (~€4.5/mes, x86_64, 4 GB RAM). Cualquier instancia te sobra — el relay es muy ligero.

## Desinstalación

```bash
sudo systemctl disable --now balchat-relay
sudo rm -rf /opt/balchat-relay /var/lib/balchat-relay /etc/systemd/system/balchat-relay.service
sudo userdel balchat-relay
```
