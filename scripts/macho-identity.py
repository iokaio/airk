# SPDX-License-Identifier: Apache-2.0
"""Pin Mach-O payload bytes while allowing only a replacement code signature."""
import hashlib
import struct


def payload_digest(data):
    if not 32 <= len(data) <= 64 << 20 or data[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("Expected bounded little-endian Mach-O 64 library")
    count, size = struct.unpack_from("<II", data, 16)
    if not 1 <= count <= 256 or not 8 * count <= size <= min(len(data) - 32, 65536):
        raise ValueError("Invalid Mach-O load commands")
    image = bytearray(data)
    offset = 32
    signature = linkedit = None
    for _ in range(count):
        if offset + 8 > 32 + size:
            raise ValueError("Truncated Mach-O load command")
        cmd, length = struct.unpack_from("<II", data, offset)
        if length < 8 or length % 8 or offset + length > 32 + size:
            raise ValueError("Invalid Mach-O command length")
        if cmd == 0x1D:
            if signature is not None or length != 16:
                raise ValueError("Invalid duplicate code signature")
            signature, signature_size = struct.unpack_from("<II", data, offset + 8)
            if signature < 32 + size or signature + signature_size != len(data):
                raise ValueError("Code signature must be the final bounded region")
            image[offset + 12:offset + 16] = bytes(4)
        elif cmd == 0x19 and data[offset + 8:offset + 24] == b"__LINKEDIT" + bytes(6):
            if linkedit is not None or length < 72:
                raise ValueError("Invalid LINKEDIT segment")
            fileoff, filesize = struct.unpack_from("<QQ", data, offset + 40)
            vmsize = struct.unpack_from("<Q", data, offset + 32)[0]
            if fileoff + filesize != len(data) or not filesize <= vmsize < filesize + 65536:
                raise ValueError("LINKEDIT does not end at file boundary")
            linkedit = offset
            image[offset + 32:offset + 40] = bytes(8)  # signature changes virtual size
            image[offset + 48:offset + 56] = bytes(8)  # and file size, never code
        offset += length
    if offset != 32 + size or signature is None or linkedit is None:
        raise ValueError("Required signature/LINKEDIT commands absent")
    return hashlib.sha256(image[:signature]).hexdigest()
