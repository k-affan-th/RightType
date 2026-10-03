#!/usr/bin/env python3
"""Build RightType's character data (2.4 B1-B3) from Unicode's own files.

Sources (Unicode License V3, see THIRD_PARTY.md), pinned:
  UnicodeData.txt  unicode-org/unicodetools  data/ucd/16.0.0/
  confusables.txt  unicode-org/unicodetools  data/security/16.0.0/
  th.xml, en.xml   unicode-org/cldr          common/annotations/  (CLDR_REF)

Usage: tools/gen_chars.py <dir with the four files>
Writes assets/chars/names.bin, assets/chars/cldr.txt, assets/chars/look.txt.

names.bin (little endian): u16 word count; per word u8 length + UTF-8;
then per character: 3-byte code point, u8 word count, u16 word index each.
Characters whose names Unicode makes up by rule (CJK, Hangul syllables,
Tangut...) and control, unassigned or private-use ones are left out: a
name like CJK UNIFIED IDEOGRAPH-4E00 finds nothing anyone would type.
"""
import collections, os, re, struct, sys

CLDR_REF = "main"
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "chars")

def main(src):
    cat, names = {}, []
    for line in open(os.path.join(src, "UnicodeData.txt"), encoding="utf-8"):
        f = line.split(";")
        cp, name, gc = int(f[0], 16), f[1], f[2]
        cat[cp] = gc
        if name.startswith("<") or gc[0] == "C" or gc == "Zl" or gc == "Zp":
            continue
        if re.match(r"(CJK COMPATIBILITY IDEOGRAPH|TANGUT|KHITAN|NUSHU|CUNEIFORM|EGYPTIAN HIEROGLYPH|ANATOLIAN HIEROGLYPH|BAMUM LETTER PHASE|LINEAR [AB] |SIGNWRITING)", name):
            continue
        names.append((cp, name.lower().split()))
    freq = collections.Counter(w for _, ws in names for w in ws)
    words = [w for w, _ in freq.most_common()]
    index = {w: i for i, w in enumerate(words)}
    assert len(words) < 65536
    out = bytearray(struct.pack("<H", len(words)))
    for w in words:
        b = w.encode()
        out += struct.pack("<B", len(b)) + b
    for cp, ws in names:
        out += cp.to_bytes(3, "little") + struct.pack("<B", len(ws))
        for w in ws:
            out += struct.pack("<H", index[w])
    open(os.path.join(OUT, "names.bin"), "wb").write(out)

    # CLDR keywords, Thai and English, per character (emoji too).
    def annotations(lang):
        t = open(os.path.join(src, f"{lang}.xml"), encoding="utf-8").read()
        kw = dict(re.findall(r'<annotation cp="([^"]+)">([^<]+)</annotation>', t))
        tts = dict(re.findall(r'<annotation cp="([^"]+)" type="tts">([^<]+)</annotation>', t))
        return kw, tts
    import html
    th, th_name = annotations("th")
    en, en_name = annotations("en")
    lines = []
    for c in sorted(set(th) | set(en), key=lambda s: [ord(x) for x in s]):
        def pack(name, kw):
            parts = [name] if name else []
            parts += [k.strip() for k in kw.split("|")] if kw else []
            seen = []
            for p in parts:
                p = html.unescape(p)
                if p and p not in seen:
                    seen.append(p)
            return "|".join(seen)
        lines.append(f"{html.unescape(c)}\t{pack(th_name.get(c), th.get(c))}\t{pack(en_name.get(c), en.get(c))}")
    open(os.path.join(OUT, "cldr.txt"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

    # Look-alikes: short ASCII → the symbols that look like it. Letters of
    # other scripts and styled maths letters are left out (they look like
    # the letter, but nobody searches for a Cyrillic o).
    rev = collections.defaultdict(list)
    for line in open(os.path.join(src, "confusables.txt"), encoding="utf-8-sig"):
        line = line.split("#")[0].strip()
        if not line:
            continue
        s, t, _ = [x.strip() for x in line.split(";")]
        s = [int(h, 16) for h in s.split()]
        t = "".join(chr(int(h, 16)) for h in t.split())
        if len(s) != 1 or not (1 <= len(t) <= 3) or not all(32 < ord(c) < 127 for c in t):
            continue
        gc = cat.get(s[0], "Cn")
        if gc[0] not in "SPN" or 0x1D400 <= s[0] <= 0x1D7FF:
            continue
        rev[t].append(chr(s[0]))
    look = [f"{k}\t{''.join(v)}" for k, v in sorted(rev.items())]
    open(os.path.join(OUT, "look.txt"), "w", encoding="utf-8").write("\n".join(look) + "\n")
    print(f"{len(names)} names, {len(words)} words, names.bin {len(out)//1024} KB; "
          f"{len(lines)} CLDR entries; {len(look)} look-alike keys")

if __name__ == "__main__":
    main(sys.argv[1])
