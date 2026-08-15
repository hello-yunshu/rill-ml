#!/usr/bin/env python3
"""Derive an Ed25519 public key from a 32-byte signing seed (RFC 8032).

Standalone and dependency-free so it can run inside any minimal Docker
container used by the RillML Docker-first release smoke. The smoke then
round-trips the derived key through `rill-pack verify`, which proves this
derivation is consistent with `ed25519-dalek`'s keypair.

Usage:
    python3 scripts/derive_ed25519_pubkey.py <64-hex-char-seed>
Output:
    64-hex-char public key
"""
import hashlib
import sys

P = 2**255 - 19
D = (-121665 * pow(121666, P - 2, P)) % P
I = pow(2, (P - 1) // 4, P)


def _xrecover(y):
    xx = (y * y - 1) * pow(D * y * y + 1, P - 2, P)
    x = pow(xx, (P + 3) // 8, P)
    if (x * x - xx) % P != 0:
        x = (x * I) % P
    if x % 2 != 0:
        x = P - x
    return x


# Base point B from RFC 8032.
_BY = 4 * pow(5, P - 2, P) % P
_BX = _xrecover(_BY)
B = (_BX % P, _BY % P)


def _add(p, q):
    (x1, y1), (x2, y2) = p, q
    if x1 is None:
        return q
    if x2 is None:
        return p
    if (x1, y1) == (x2, y2):
        return _dbl(p)
    if x1 == x2 and y1 == P - y2:
        return (None, None)
    z = D * x1 * x2 * y1 * y2 % P
    x3 = (x1 * y2 + x2 * y1) * pow(1 + z, P - 2, P) % P
    y3 = (y1 * y2 + x1 * x2) * pow(1 - z, P - 2, P) % P
    return (x3, y3)


def _dbl(p):
    (x1, y1) = p
    if x1 is None:
        return (None, None)
    z = D * x1 * x1 * y1 * y1 % P
    x3 = (2 * x1 * y1) * pow(1 + z, P - 2, P) % P
    y3 = (y1 * y1 + x1 * x1) * pow(1 - z, P - 2, P) % P
    return (x3, y3)


def _scalarmult(p, n):
    q = (None, None)
    while n > 0:
        if n & 1:
            q = _add(q, p)
        p = _dbl(p)
        n >>= 1
    return q


def _encodepoint(p):
    (x, y) = p
    bits = [(y >> i) & 1 for i in range(255)] + [x & 1]
    return bytes(
        sum(bits[i * 8 + j] << j for j in range(8)) for i in range(32)
    )


def public_key(seed_bytes):
    h = hashlib.sha512(seed_bytes).digest()
    a = int.from_bytes(h[:32], "little")
    # RFC 8032 clamping.
    a &= (1 << 254) - 8
    a &= ~(1 << 255)
    a |= 1 << 254
    return _encodepoint(_scalarmult(B, a)).hex()


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: derive_ed25519_pubkey.py <64-hex-char-seed>")
    seed_hex = sys.argv[1]
    if len(seed_hex) != 64:
        sys.exit("seed must be exactly 64 hex characters (32 bytes)")
    try:
        seed = bytes.fromhex(seed_hex)
    except ValueError:
        sys.exit("seed is not valid hex")
    print(public_key(seed))


if __name__ == "__main__":
    main()