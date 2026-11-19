#!/usr/bin/env python3
"""Extract translatable messages from quai's Rust and QML sources into a .pot.

xgettext only reads Rust from gettext 0.24 on, so this small extractor knows
the keywords of the quai-i18n crate, which QML reaches through the I18n
singleton with the same names:

    tr("msg")    trf("msg {x}", ...)    n_("msg")
    trc("context", "msg")
    trn("{n} singular", "{n} plural", n)    trnf(...)

The message arguments must be string literals. A comment that starts with
"Translators:" on the lines just above a call becomes an extracted comment.
Messages with {name} placeholders get the python-brace-format flag, so
`msgfmt --check` rejects a translation that loses one.

Usage: extract.py OUTPUT.pot FILE...
"""

import re
import sys
from collections import OrderedDict

KEYWORDS = {  # name -> roles of the leading string arguments
    "tr": ("msgid",),
    "trf": ("msgid",),
    "n_": ("msgid",),
    "trc": ("msgctxt", "msgid"),
    "trn": ("msgid", "msgid_plural"),
    "trnf": ("msgid", "msgid_plural"),
}
HEADER = r"""# Translations of quai, the RPM release tracker.
# This file is distributed under the same license as quai (Apache-2.0).
#
#, fuzzy
msgid ""
msgstr ""
"Project-Id-Version: quai\n"
"Report-Msgid-Bugs-To: https://github.com/IT-Yohan/quai/issues\n"
"Language: \n"
"MIME-Version: 1.0\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"
"Plural-Forms: nplurals=INTEGER; plural=EXPRESSION;\n"

"""
PLACEHOLDER = re.compile(r"\{[A-Za-z_][A-Za-z0-9_]*\}")


class Token:
    __slots__ = ("kind", "value", "line")

    def __init__(self, kind, value, line):
        self.kind, self.value, self.line = kind, value, line


def decode_escape(text, i, js):
    """Decode the escape starting after the backslash at text[i]; return (str, next index)."""
    c = text[i]
    simple = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", "'": "'", '"': '"', "0": "\0"}
    if js:
        simple.update({"b": "\b", "f": "\f", "v": "\v"})
    if c in simple:
        return simple[c], i + 1
    if c == "\n":  # line continuation; Rust also skips the next line's indentation
        i += 1
        if not js:
            while i < len(text) and text[i] in " \t\r\n":
                i += 1
        return "", i
    if c == "x":
        return chr(int(text[i + 1 : i + 3], 16)), i + 3
    if c == "u":
        if text[i + 1] == "{":
            end = text.index("}", i)
            return chr(int(text[i + 2 : end], 16)), end + 1
        return chr(int(text[i + 1 : i + 5], 16)), i + 5
    return c, i + 1


def tokenize(text, js):
    """Yield identifiers, strings, punctuation and comments, with line numbers."""
    i, line, n = 0, 1, len(text)
    prev = None  # last significant token, to tell a JS regex from a division
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif c in " \t\r":
            i += 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            yield Token("comment", text[i + 2 : end].strip(), line)
            i = end
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j) and not js:
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            body = text[i + 2 : j - 2]
            yield Token("comment", " ".join(l.strip(" *") for l in body.splitlines()).strip(), line)
            line += body.count("\n")
            i = j
        elif not js and re.match(r'(b?r)(#*)"', text[i:]):  # Rust raw string
            m = re.match(r'(b?r)(#*)"', text[i:])
            close = '"' + m.group(2)
            start = i + m.end()
            end = text.index(close, start)
            value = text[start:end]
            tok = Token("string" if not m.group(1).startswith("b") else "other", value, line)
            line += value.count("\n")
            i = end + len(close)
            prev = tok
            yield tok
        elif c == '"' or (js and c == "'") or (not js and c == "b" and text.startswith('b"', i)):
            if c == "b":
                i += 1
            quote, j, out, start_line = text[i], i + 1, [], line
            while text[j] != quote:
                if text[j] == "\\":
                    if text[j + 1] == "\n":
                        line += 1
                    s, j = decode_escape(text, j + 1, js)
                    out.append(s)
                else:
                    if text[j] == "\n":
                        line += 1
                    out.append(text[j])
                    j += 1
            tok = Token("string" if c != "b" else "other", "".join(out), start_line)
            i = j + 1
            prev = tok
            yield tok
        elif js and c == "`":  # template literal: never a message
            j = i + 1
            while text[j] != "`":
                j += 2 if text[j] == "\\" else 1
            line += text[i:j].count("\n")
            i = j + 1
            prev = Token("other", "", line)
        elif not js and c == "'":  # char literal or lifetime
            m = re.match(r"'(\\(x..|u\{[0-9a-fA-F]+\}|.)|[^\\'])'", text[i:])
            i += m.end() if m else 1
            prev = Token("other", "", line)
        elif js and c == "/" and (prev is None or (prev.kind == "punct" and prev.value in "(,=:[!&|?{};+-*%<>~^")
                                  or (prev.kind == "ident" and prev.value in ("return", "typeof", "case"))):
            j, in_class = i + 1, False  # regex literal
            while text[j] != "/" or in_class:
                if text[j] == "\\":
                    j += 1
                elif text[j] == "[":
                    in_class = True
                elif text[j] == "]":
                    in_class = False
                j += 1
            i = j + 1
            while i < n and text[i].isalpha():
                i += 1
            prev = Token("other", "", line)
        elif c.isalpha() or c == "_":
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[i:])
            tok = Token("ident", m.group(0), line)
            i += m.end()
            prev = tok
            yield tok
        else:
            tok = Token("punct", c, line)
            i += 1
            prev = tok
            yield tok


def extract(path):
    text = open(path, encoding="utf-8").read()
    toks = list(tokenize(text, js=path.endswith((".qml", ".js"))))
    found, used_notes = [], set()
    for k, tok in enumerate(toks):
        if tok.kind != "ident" or tok.value not in KEYWORDS:
            continue
        if k + 1 >= len(toks) or toks[k + 1].value != "(" or (k > 0 and toks[k - 1].value in ("fn", "function")):
            continue
        roles, args, j = KEYWORDS[tok.value], [], k + 2
        while len(args) < len(roles) and j < len(toks):
            while j < len(toks) and toks[j].kind == "comment":
                j += 1
            if toks[j].kind != "string" or toks[j + 1].value not in (",", ")"):
                break
            args.append(toks[j].value)
            j += 2
        if len(args) != len(roles):
            continue  # a variable, e.g. tr(entry) after n_(): nothing to extract here
        notes = []
        back = k - 1
        while back >= 0 and toks[back].kind != "comment" and toks[back].line == tok.line:
            back -= 1
        last_line = tok.line
        while back >= 0 and toks[back].kind == "comment" and toks[back].line >= last_line - 1:
            if back not in used_notes:  # a note belongs to the first call after it
                notes.insert(0, toks[back].value)
                used_notes.add(back)
            last_line = toks[back].line
            back -= 1
        note = " ".join(notes)
        idx = note.find("Translators:")
        found.append((dict(zip(roles, args)), f"{path}:{tok.line}", note[idx:] if idx >= 0 else ""))
    return found


def po_string(s):
    s = s.replace("\\", "\\\\").replace('"', '\\"').replace("\t", "\\t").replace("\r", "\\r")
    if "\n" not in s.rstrip("\n"):
        return '"' + s.replace("\n", "\\n") + '"'
    parts = s.split("\n")
    lines = [p + "\\n" for p in parts[:-1]] + ([parts[-1]] if parts[-1] else [])
    return '""\n' + "\n".join(f'"{l}"' for l in lines)


def main():
    out, files = sys.argv[1], sys.argv[2:]
    entries = OrderedDict()
    for path in files:
        for msg, ref, note in extract(path):
            key = (msg.get("msgctxt"), msg["msgid"])
            e = entries.setdefault(key, {"msg": msg, "refs": [], "notes": []})
            if msg.get("msgid_plural") and e["msg"].get("msgid_plural", msg["msgid_plural"]) != msg["msgid_plural"]:
                sys.exit(f"{ref}: same message with two plural forms")
            e["msg"].update(msg)
            e["refs"].append(ref)
            if note and note not in e["notes"]:
                e["notes"].append(note)
    with open(out, "w", encoding="utf-8") as f:
        f.write(HEADER)
        for e in entries.values():
            msg = e["msg"]
            for note in e["notes"]:
                f.write(f"#. {note}\n")
            f.write("#: " + " ".join(e["refs"]) + "\n")
            if any(PLACEHOLDER.search(msg.get(r, "")) for r in ("msgid", "msgid_plural")):
                f.write("#, python-brace-format\n")
            if msg.get("msgctxt") is not None:
                f.write(f"msgctxt {po_string(msg['msgctxt'])}\n")
            f.write(f"msgid {po_string(msg['msgid'])}\n")
            if "msgid_plural" in msg:
                f.write(f"msgid_plural {po_string(msg['msgid_plural'])}\n")
                f.write('msgstr[0] ""\nmsgstr[1] ""\n\n')
            else:
                f.write('msgstr ""\n\n')
    print(f"{out}: {len(entries)} messages from {len(files)} files", file=sys.stderr)


if __name__ == "__main__":
    main()
