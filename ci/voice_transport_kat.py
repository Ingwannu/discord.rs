#!/usr/bin/env python3
"""Known-answer vectors for Discord voice transport encryption (rtpsize AEAD modes).

Prints the packets that the `voice_runtime` tests in `src/voice_runtime.rs` pin as
constants. The vectors are computed here, independently of the crate's Rust AEAD code:
AES-256-GCM and ChaCha20-Poly1305 come from the Python `cryptography` package, and
XChaCha20-Poly1305 is built from a small HChaCha20 implementation below (checked against
the draft-irtf-cfrg-xchacha-03 test vectors before anything is printed).

The packet layout follows @discordjs/voice (`Networking.ts` `encryptOpusPacket`,
`VoiceReceiver.ts` `parsePacket`) and Songbird (`driver/crypto.rs`):

- AEAD nonce: the 32-bit packet counter, big-endian, in the FIRST four bytes of a zeroed
  12-byte (AES-GCM) or 24-byte (XChaCha20) nonce.
- Packet: cleartext header || ciphertext || 16-byte tag || the same four counter bytes.
- Additional data: the fixed RTP header, the CSRCs and, when the X bit is set, only the
  4-byte extension preamble (profile + length). The extension body is encrypted together
  with the Opus payload (Discord docs, Voice Connections > Transport Encryption Modes:
  "considers CSRCs and (optionally) the extension preamble to be part of the unencrypted
  header").
- Padding: with the P bit set, the payload ends with padding whose last octet counts the
  padding octets, itself included (RFC 3550 section 5.1). It is part of the plaintext, so
  it is encrypted too; @discordjs/voice strips it after decrypting.

Usage: python3 ci/voice_transport_kat.py   (needs `cryptography`, e.g. `pip install cryptography`)
"""

import struct

from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305


def _rotl32(value, shift):
    return ((value << shift) | (value >> (32 - shift))) & 0xFFFFFFFF


def _quarter_round(state, a, b, c, d):
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = _rotl32(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = _rotl32(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = _rotl32(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = _rotl32(state[b] ^ state[c], 7)


def hchacha20(key, nonce16):
    """HChaCha20 (draft-irtf-cfrg-xchacha-03 section 2.2)."""
    state = list(struct.unpack("<4I", b"expand 32-byte k"))
    state += list(struct.unpack("<8I", key))
    state += list(struct.unpack("<4I", nonce16))
    for _ in range(10):
        _quarter_round(state, 0, 4, 8, 12)
        _quarter_round(state, 1, 5, 9, 13)
        _quarter_round(state, 2, 6, 10, 14)
        _quarter_round(state, 3, 7, 11, 15)
        _quarter_round(state, 0, 5, 10, 15)
        _quarter_round(state, 1, 6, 11, 12)
        _quarter_round(state, 2, 7, 8, 13)
        _quarter_round(state, 3, 4, 9, 14)
    return struct.pack("<8I", *(state[0:4] + state[12:16]))


def xchacha20poly1305_encrypt(key, nonce24, plaintext, aad):
    """XChaCha20-Poly1305 (draft-irtf-cfrg-xchacha-03 section 2.3)."""
    subkey = hchacha20(key, nonce24[:16])
    return ChaCha20Poly1305(subkey).encrypt(b"\x00" * 4 + nonce24[16:], plaintext, aad)


def _check_xchacha_against_draft():
    # Section 2.2.1: HChaCha20 test vector.
    key = bytes(range(32))
    nonce = bytes.fromhex("000000090000004a0000000031415927")
    expected = bytes.fromhex(
        "82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc"
    )
    assert hchacha20(key, nonce) == expected, "HChaCha20 does not match the draft"

    # Appendix A.3.1: AEAD_XChaCha20_Poly1305 test vector.
    plaintext = (
        b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip "
        b"for the future, sunscreen would be it."
    )
    aad = bytes.fromhex("50515253c0c1c2c3c4c5c6c7")
    key = bytes(range(0x80, 0xA0))
    nonce = bytes.fromhex("404142434445464748494a4b4c4d4e4f5051525354555657")
    expected = bytes.fromhex(
        "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb"
        "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452"
        "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9"
        "21f9664c97637da9768812f615c68b13b52e"
        "c0875924c1c7987947deafd8780acf49"
    )
    assert xchacha20poly1305_encrypt(key, nonce, plaintext, aad) == expected, (
        "XChaCha20-Poly1305 does not match the draft"
    )


def rtp_header(first_byte, sequence, timestamp, ssrc, csrcs=(), extension=None):
    """Returns (cleartext AAD, encrypted extension body) for one RTP header."""
    header = bytes([first_byte | len(csrcs), 0x78])
    header += struct.pack(">HII", sequence, timestamp, ssrc)
    for csrc in csrcs:
        header += struct.pack(">I", csrc)
    body = b""
    if extension is not None:
        profile, body = extension
        assert len(body) % 4 == 0
        header += struct.pack(">HH", profile, len(body) // 4)
    return header, body


def seal(mode, key, aad, plaintext, counter):
    counter_bytes = struct.pack(">I", counter)
    if mode == "aes":
        nonce = counter_bytes + b"\x00" * 8
        sealed = AESGCM(key).encrypt(nonce, plaintext, aad)
    else:
        nonce = counter_bytes + b"\x00" * 20
        sealed = xchacha20poly1305_encrypt(key, nonce, plaintext, aad)
    return aad + sealed + counter_bytes


def rust_bytes(name, data):
    lines = [f"const {name}: [u8; {len(data)}] = ["]
    for start in range(0, len(data), 12):
        chunk = ", ".join(f"0x{byte:02x}" for byte in data[start : start + 12])
        lines.append(f"    {chunk},")
    lines.append("];")
    return "\n".join(lines)


def main():
    _check_xchacha_against_draft()

    key = bytes(range(0x20, 0x40))
    opus = bytes([0xF8, 0xFF, 0xFE])

    # Outbound shape: what `encrypt_transport_payload` emits (no CSRCs, no extension).
    aad, _ = rtp_header(0x80, 0x1234, 0x5678_9ABC, 0xDEAD_BEEF)
    counter = 0x0102_0304
    print(rust_bytes("KAT_AES_PLAIN", seal("aes", key, aad, opus, counter)))
    print(rust_bytes("KAT_XCHACHA_PLAIN", seal("xchacha", key, aad, opus, counter)))

    # Inbound shape: two CSRCs and a one-word RFC 8285 one-byte-header extension
    # (ID 1, one data byte 0xAB, two padding bytes), with the X bit set.
    aad, body = rtp_header(
        0x90,
        0x0102,
        0x0A0B_0C0D,
        0x1122_3344,
        csrcs=(0xCAFE_BABE, 0x0BAD_F00D),
        extension=(0xBEDE, bytes([0x10, 0xAB, 0x00, 0x00])),
    )
    counter = 0xA1B2_C3D4
    print(rust_bytes("KAT_AES_EXTENDED", seal("aes", key, aad, body + opus, counter)))
    print(
        rust_bytes("KAT_XCHACHA_EXTENDED", seal("xchacha", key, aad, body + opus, counter))
    )

    # Padded shape: P and X bits, one CSRC, a one-word extension (ID 1, data 0xCD), and
    # four octets of padding after the Opus frame (three zeros, then the count 4).
    aad, body = rtp_header(
        0xB0,
        0x0304,
        0x0E0F_1011,
        0x5566_7788,
        csrcs=(0xFEED_FACE,),
        extension=(0xBEDE, bytes([0x10, 0xCD, 0x00, 0x00])),
    )
    padding = bytes([0x00, 0x00, 0x00, 0x04])
    counter = 0x5A6B_7C8D
    plaintext = body + opus + padding
    print(rust_bytes("KAT_AES_PADDED", seal("aes", key, aad, plaintext, counter)))
    print(rust_bytes("KAT_XCHACHA_PADDED", seal("xchacha", key, aad, plaintext, counter)))


if __name__ == "__main__":
    main()
