"""Check the Support page: decode the two hard-coded 33x33 QR matrices in
src/settings.rs, compare them with the addresses beside them and verify their
error-correction codewords. Stdlib only; reads, never writes.

Usage: python -I tools/check_support_qr.py src/settings.rs
"""

import re
import sys

src = open(sys.argv[1], encoding="utf-8").read()


def matrix(name):
    body = re.search(rf"pub const {name}: \[u64; 33\] = \[(.*?)\];", src, re.S).group(1)
    rows = [int(x.replace("_", ""), 0) for x in re.findall(r"0x[0-9A-Fa-f_]+|\b\d[\d_]*\b", body)]
    assert len(rows) == 33, (name, len(rows))
    return [[(rows[r] >> c) & 1 for c in range(33)] for r in range(33)]


def function_mask(n=33):
    f = [[False] * n for _ in range(n)]

    def box(r0, c0, h, w):
        for r in range(r0, r0 + h):
            for c in range(c0, c0 + w):
                if 0 <= r < n and 0 <= c < n:
                    f[r][c] = True

    box(0, 0, 9, 9)          # top-left finder + separator + format
    box(0, n - 8, 9, 8)      # top-right
    box(n - 8, 0, 8, 9)      # bottom-left
    for i in range(n):
        f[6][i] = True
        f[i][6] = True       # timing
    box(26 - 2, 26 - 2, 5, 5)  # version 4 alignment pattern centre (26, 26)
    return f


MASKS = [
    lambda r, c: (r + c) % 2 == 0,
    lambda r, c: r % 2 == 0,
    lambda r, c: c % 3 == 0,
    lambda r, c: (r + c) % 3 == 0,
    lambda r, c: (r // 2 + c // 3) % 2 == 0,
    lambda r, c: (r * c) % 2 + (r * c) % 3 == 0,
    lambda r, c: ((r * c) % 2 + (r * c) % 3) % 2 == 0,
    lambda r, c: ((r + c) % 2 + (r * c) % 3) % 2 == 0,
]
# (data codewords per block, number of blocks) for version 4, by EC level bits
# version 4 (100 codewords): (codewords per block, data codewords per block)
V4 = {0b01: ("L", [(100, 80)]), 0b00: ("M", [(50, 32)] * 2),
      0b11: ("Q", [(50, 24)] * 2), 0b10: ("H", [(25, 9)] * 4)}
FORMAT_XOR = 0b101010000010010


def decode(m):
    n = 33
    # format bits: row 8 / column 8 near the top-left finder
    bits = [m[8][c] for c in (0, 1, 2, 3, 4, 5, 7, 8)] + [m[r][8] for r in (7, 5, 4, 3, 2, 1, 0)]
    fmt = int("".join(map(str, bits)), 2) ^ FORMAT_XOR
    ec, mask = (fmt >> 13) & 3, (fmt >> 10) & 7
    level, blocks = V4[ec]
    func = function_mask()
    bitstream = []
    c = n - 1
    up = True
    while c > 0:
        if c == 6:
            c -= 1
        rows = range(n - 1, -1, -1) if up else range(n)
        for r in rows:
            for cc in (c, c - 1):
                if not func[r][cc]:
                    bitstream.append(m[r][cc] ^ (1 if MASKS[mask](r, cc) else 0))
        up = not up
        c -= 2
    words = [int("".join(map(str, bitstream[i:i + 8])), 2) for i in range(0, len(bitstream) - 7, 8)]
    # de-interleave the data codewords
    data_counts = [d for _, d in blocks]
    data = [[] for _ in blocks]
    idx = 0
    for k in range(max(data_counts)):
        for b, cnt in enumerate(data_counts):
            if k < cnt:
                data[b].append(words[idx])
                idx += 1
    flat = [w for blk in data for w in blk]
    bits = "".join(f"{w:08b}" for w in flat)
    mode = int(bits[:4], 2)
    if mode == 0b0100:  # byte
        count = int(bits[4:12], 2)
        text = bytes(int(bits[12 + 8 * i:20 + 8 * i], 2) for i in range(count)).decode("latin-1")
    else:
        text = f"<mode {mode:04b} not handled>"
    return level, mask, text


for name, want_const in (("QR_LIGHTNING", "BTC_LIGHTNING"), ("QR_BITCOIN", "BTC_MAINNET")):
    want = re.search(rf'{want_const}: &str = "([^"]+)"', src).group(1)
    level, mask, got = decode(matrix(name))
    print(f"{name}: EC {level}, mask {mask}, decodes to: {got!r}")
    print(f"  address in code: {want!r}")
    for form, ok in (("exact", got == want), ("with scheme prefix", got.split(":", 1)[-1] == want), ("case-insensitive", got.lower().split(":", 1)[-1] == want.lower())):
        if ok:
            print(f"  => MATCH ({form})")
            break
    else:
        print("  => DOES NOT MATCH")


# ---- Reed-Solomon check: recompute the error-correction codewords --------
EXP, LOG = [0] * 512, [0] * 256
x = 1
for i in range(255):
    EXP[i], LOG[x] = x, i
    x <<= 1
    if x & 0x100:
        x ^= 0x11D
for i in range(255, 512):
    EXP[i] = EXP[i - 255]


def gmul(a, b):
    return 0 if a == 0 or b == 0 else EXP[LOG[a] + LOG[b]]


def rs_remainder(data, nsym):
    gen = [1]
    for i in range(nsym):
        nxt = [0] * (len(gen) + 1)
        for j, g in enumerate(gen):
            nxt[j] ^= g
            nxt[j + 1] ^= gmul(g, EXP[i])
        gen = nxt
    rem = [0] * nsym
    for d in data:
        factor = d ^ rem[0]
        rem = rem[1:] + [0]
        for j in range(nsym):
            rem[j] ^= gmul(gen[j + 1], factor)
    return rem


def codewords(m):
    n = 33
    bits = [m[8][c] for c in (0, 1, 2, 3, 4, 5, 7, 8)] + [m[r][8] for r in (7, 5, 4, 3, 2, 1, 0)]
    fmt = int("".join(map(str, bits)), 2) ^ FORMAT_XOR
    ec, mask = (fmt >> 13) & 3, (fmt >> 10) & 7
    func, stream, c, up = function_mask(), [], n - 1, True
    while c > 0:
        if c == 6:
            c -= 1
        for r in (range(n - 1, -1, -1) if up else range(n)):
            for cc in (c, c - 1):
                if not func[r][cc]:
                    stream.append(m[r][cc] ^ (1 if MASKS[mask](r, cc) else 0))
        up, c = not up, c - 2
    return ec, [int("".join(map(str, stream[i:i + 8])), 2) for i in range(0, len(stream) - 7, 8)]


for name in ("QR_LIGHTNING", "QR_BITCOIN"):
    ec, words = codewords(matrix(name))
    level, blocks = V4[ec]
    counts = [d for _, d in blocks]
    ecn = blocks[0][0] - blocks[0][1]
    data = [[] for _ in blocks]
    idx = 0
    for k in range(max(counts)):
        for b, cnt in enumerate(counts):
            if k < cnt:
                data[b].append(words[idx]); idx += 1
    ecc = [[] for _ in blocks]
    for k in range(ecn):
        for b in range(len(blocks)):
            ecc[b].append(words[idx]); idx += 1
    good = all(rs_remainder(data[b], ecn) == ecc[b] for b in range(len(blocks)))
    print(f"{name}: error-correction codewords {'CORRECT' if good else 'WRONG'} ({len(blocks)} blocks x {ecn} ecc)")
