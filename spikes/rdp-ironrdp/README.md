# Spike: IronRDP as the RDP stack

Throwaway spike, not product code. It answers one question: can IronRDP replace
the mstsc ActiveX control that the C# Heimdall hosts, on Linux first.

## Contents

| File | Purpose |
|---|---|
| `screenshot-example.patch` | Applies to IronRDP `9b151c4`. Makes the `screenshot` example accept TLS-only servers (`SPIKE_TLS`), send `INFO_AUTOLOGON` (`SPIKE_AUTOLOGON`), and wait 12 s instead of 3 s before capturing |
| `rdp_nego_probe.py` | Sends no credentials. Reports the security protocol a server selects, and its TLS version |

## Reproduce

On a Linux host (measured on WSL AlmaLinux 9.7), with `gcc`, `cmake` and rustup:

```bash
git clone --depth 1 https://github.com/Devolutions/IronRDP.git
cd IronRDP
git apply ../screenshot-example.patch
cargo build --release -p ironrdp --example screenshot --features "session connector graphics"
```

Negotiation only, on the same Linux host:

```bash
python3 rdp_nego_probe.py <HOST>[:<PORT>]
```

Full connection to an NLA server, on the same Linux host. The password is read
from the terminal so it lands neither in the shell history nor in this file:

```bash
read -rs RDP_PASSWORD; ./target/release/examples/screenshot --host <HOST> -u <USER> -p "$RDP_PASSWORD" -o /tmp/nla.png; unset RDP_PASSWORD
```

## Results

Recorded in the project runbook, file `30-rdp.md`.
