#!/usr/bin/env python3
"""Regenerates the byte-exact fixtures in tests/fixtures.

The fixtures are committed as they are (see .gitattributes); run this only to add new ones and
review the diff. File names: <encoding>[-bom]-<eol>.txt.
"""

from pathlib import Path

HERE = Path(__file__).parent / "fixtures"

UNICODE_TEXT = [
    "Birchpad test: The quick brown fox jumps over the lazy dog.",
    "Русский текст: съешь же ещё этих мягких французских булок.",
    "日本語のテキスト, emoji 😀 and accents: café, naïve, Grüße.",
    "\tIndented line with a tab",
    "",
    "Last line",
]

# Mostly ASCII, so that UTF-16 without a BOM is recognizable by its zero bytes.
ASCII_HEAVY = [
    "Plain English text that is long enough to show the zero byte pattern.",
    "Second line, still plain; one word in Russian: привет.",
    "Third line.",
]

RUSSIAN = [
    "Съешь же ещё этих мягких французских булок, да выпей чаю.",
    "В чащах юга жил бы цитрус? Да, но фальшивый экземпляр!",
    "Широкая электрификация южных губерний даст мощный толчок подъёму сельского хозяйства.",
    "Эх, чужак, общий съём цен шляп (юфть) - вдрызг!",
]

WESTERN = [
    "Voix ambiguë d'un cœur qui, au zéphyr, préfère les jattes de kiwis.",
    "Zwölf Boxkämpfer jagen Viktor quer über den großen Sylter Deich.",
    "El pingüino Wenceslao hizo kilómetros bajo exhaustiva lluvia y frío, añoraba.",
]

JAPANESE = [
    "いろはにほへと ちりぬるを わかよたれそ つねならむ",
    "うゐのおくやま けふこえて あさきゆめみし ゑひもせす",
    "日本語の文章とEnglishが混在しています。",
    "今日は良い天気ですね。明日も晴れるでしょうか。",
    "東京都は日本の首都です。人口は約千四百万人です。",
]

EOLS = {"crlf": "\r\n", "lf": "\n", "cr": "\r"}

FIXTURES = [
    # (name, python codec, bom bytes, lines)
    ("utf-8", "utf-8", b"", UNICODE_TEXT),
    ("utf-8-bom", "utf-8", b"\xef\xbb\xbf", UNICODE_TEXT),
    ("utf-16le-bom", "utf-16-le", b"\xff\xfe", UNICODE_TEXT),
    ("utf-16be-bom", "utf-16-be", b"\xfe\xff", UNICODE_TEXT),
    ("utf-16le", "utf-16-le", b"", ASCII_HEAVY),
    ("utf-16be", "utf-16-be", b"", ASCII_HEAVY),
    ("windows-1251", "cp1251", b"", RUSSIAN),
    ("koi8-r", "koi8-r", b"", RUSSIAN),
    ("ibm866", "cp866", b"", RUSSIAN),
    ("windows-1252", "cp1252", b"", WESTERN),
    ("shift_jis", "shift_jis", b"", JAPANESE),
]


def main() -> None:
    HERE.mkdir(exist_ok=True)
    for name, codec, bom, lines in FIXTURES:
        for eol_name, eol in EOLS.items():
            text = eol.join(lines) + eol
            (HERE / f"{name}-{eol_name}.txt").write_bytes(bom + text.encode(codec))

    special = {
        "empty.txt": b"",
        "bom-only.txt": b"\xef\xbb\xbf",
        "utf-16le-bom-only.txt": b"\xff\xfe",
        "mixed-eol.txt": b"crlf\r\nlf\ncr\rlast",
        "no-final-eol.txt": b"one\r\ntwo",
        # "ok", an invalid byte, then valid UTF-8.
        "invalid-utf8.txt": b"ok \xc3\x28 and then \xd0\xb6 fine\n",
        # UTF-16 LE with BOM and an odd number of bytes.
        "utf-16le-truncated.txt": b"\xff\xfeh\x00i\x00!",
        # UTF-16 LE with BOM and an unpaired high surrogate.
        "utf-16le-lone-surrogate.txt": b"\xff\xfea\x00\x00\xd8b\x00",
        "nul-bytes.txt": b"a\x00b\x00\x00c\n",
    }
    for name, data in special.items():
        (HERE / name).write_bytes(data)


if __name__ == "__main__":
    main()
