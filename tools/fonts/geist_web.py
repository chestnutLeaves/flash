"""Cuts the web's second Geist files from the full font.

The site serves Geist in two parts per face, each with the characters
it holds (`unicode-range`), so a page loads only what its text needs:

- the core files (`Geist-Regular.woff2` and the rest), the Western
  Latin subset the site has always served, left as they are;
- the extended files (`Geist-Regular-ext.woff2`), every other character
  the full font holds (Latin Extended, Vietnamese, Cyrillic), cut here
  from the release's TTF. Geist's six Greek letters are left out: a
  Greek word would be set in two faces, so Greek takes the system's own
  font (`flash_core::locale::Script::style`).

It writes the extended files and `geist-ranges.json`, the ranges of
both parts, which the stylesheets' `@font-face` rules and the coverage
test read. Geist is under the SIL Open Font License 1.1 with no Reserved
Font Name, so a subset keeps the name (OFL FAQ 5.6).

    python tools/fonts/geist_web.py <dir with Geist-*.ttf and GeistMono-*.ttf> crates/flash-server/static/fonts

Needs fontTools and brotli (`pip install fonttools brotli`).
"""

import json
import pathlib
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

FACES = [
    "Geist-Regular",
    "Geist-Medium",
    "Geist-SemiBold",
    "GeistMono-Regular",
    "GeistMono-Medium",
    "GeistMono-SemiBold",
]

# Greek and Coptic: Geist holds six of its letters, not the language.
GREEK = range(0x0370, 0x0400)


def ranges(codepoints):
    """`codepoints` as a CSS unicode-range: U+20-7E, U+A0, ..."""
    spans = []
    for cp in sorted(codepoints):
        if spans and cp == spans[-1][1] + 1:
            spans[-1][1] = cp
        else:
            spans.append([cp, cp])
    return ",".join(f"U+{a:X}" if a == b else f"U+{a:X}-{b:X}" for a, b in spans)


def main(source, out):
    source, out = pathlib.Path(source), pathlib.Path(out)
    record = {}
    for face in FACES:
        core = set(TTFont(out / f"{face}.woff2").getBestCmap())
        full = set(TTFont(source / f"{face}.ttf").getBestCmap())
        missing = core - full
        if missing:
            sys.exit(f"{face}: the core file holds characters the TTF lacks: {sorted(missing)[:10]}")
        ext = sorted(cp for cp in full - core if cp not in GREEK)
        options = subset.Options()
        options.flavor = "woff2"
        options.layout_features = ["*"]
        options.name_IDs = ["*"]
        options.name_languages = ["*"]
        options.hinting = False
        options.notdef_outline = True
        # The release's own timestamp, so the same font cuts to the same
        # bytes every time.
        font = TTFont(source / f"{face}.ttf", recalcTimestamp=False)
        cutter = subset.Subsetter(options=options)
        cutter.populate(unicodes=ext)
        cutter.subset(font)
        font.flavor = "woff2"
        font.save(out / f"{face}-ext.woff2")
        record[face] = {"core": ranges(core), "ext": ranges(ext)}
        print(f"{face}: core {len(core)}, ext {len(ext)}")
    # Bytes, so the file is the same on every system (no CRLF on Windows).
    (out / "geist-ranges.json").write_bytes((json.dumps(record, indent=2) + "\n").encode("utf-8"))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
