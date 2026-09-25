#!/usr/bin/env python3
#
# Copyright 2026 Julien Bombled
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
"""Credential-free RDP negotiation probe.

Sends an X.224 Connection Request advertising SSL | HYBRID | HYBRID_EX, reports
the security protocol the server selects, then completes a TLS handshake and
prints the TLS version and cipher. Sends no credentials.

Usage: rdp_nego_probe.py HOST[:PORT] [HOST[:PORT] ...]
"""
import socket
import ssl
import struct
import sys

DEFAULT_PORT = 3389
TIMEOUT_S = 5
RECV_BUFFER = 1024

PROTOCOL_SSL = 0x1
PROTOCOL_HYBRID = 0x2
PROTOCOL_HYBRID_EX = 0x8
PROTOCOL_NAMES = {
    PROTOCOL_SSL: "SSL",
    PROTOCOL_HYBRID: "HYBRID",
    0x4: "RDSTLS",
    PROTOCOL_HYBRID_EX: "HYBRID_EX",
    0x10: "RDSAAD",
}
REQUESTED = PROTOCOL_SSL | PROTOCOL_HYBRID | PROTOCOL_HYBRID_EX

TPKT_VERSION = 3
X224_CONNECTION_REQUEST = 0xE0
NEG_TYPE_REQUEST = 0x01
NEG_TYPE_FAILURE = 0x03
NEG_LENGTH = 8
NEG_OFFSET = 11


def connection_request() -> bytes:
    neg_req = struct.pack("<BBHI", NEG_TYPE_REQUEST, 0, NEG_LENGTH, REQUESTED)
    x224 = bytes([len(neg_req) + 6, X224_CONNECTION_REQUEST, 0, 0, 0, 0, 0]) + neg_req
    return struct.pack(">BBH", TPKT_VERSION, 0, len(x224) + 4) + x224


def probe(host: str, port: int) -> None:
    sock = socket.create_connection((host, port), timeout=TIMEOUT_S)
    sock.sendall(connection_request())
    data = sock.recv(RECV_BUFFER)
    neg = data[NEG_OFFSET:NEG_OFFSET + NEG_LENGTH]
    if len(neg) < NEG_LENGTH:
        print(f"{host}:{port} short response {data.hex()}")
        return
    kind, _flags, _length, value = struct.unpack("<BBHI", neg)
    if kind == NEG_TYPE_FAILURE:
        print(f"{host}:{port} negotiation FAILURE code={value}")
        return
    names = [name for bit, name in PROTOCOL_NAMES.items() if value & bit] or ["STANDARD_RDP"]
    print(f"{host}:{port} selected={'|'.join(names)} (0x{value:x})")
    if value == 0:
        return
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    with ctx.wrap_socket(sock, server_hostname=host) as tls:
        der = tls.getpeercert(binary_form=True)
        print(f"  tls={tls.version()} cipher={tls.cipher()[0]} cert_der_bytes={len(der)}")


def main(targets: list[str]) -> None:
    for target in targets:
        host, _, port = target.partition(":")
        try:
            probe(host, int(port or DEFAULT_PORT))
        except OSError as exc:
            print(f"{target} error: {exc}")


if __name__ == "__main__":
    main(sys.argv[1:])
