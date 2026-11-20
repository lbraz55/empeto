#!/usr/bin/env python3
"""Catálogo de traduções do EMPETO (documento de evolução v1.1, 4.1).

Uso:
  scripts/i18n.py check            lista frases sem tradução (sai com erro se houver)
  scripts/i18n.py update           extrai frases de ui/ e src/ e atualiza os .po (mantém traduções)
  scripts/i18n.py merge ARQ.json   aplica traduções {"msgid": {"en": "...", "es": "..."}} e atualiza

O texto-fonte é português (pt-BR). Fontes de frases:
  - Slint: @tr("...")
  - Rust:  tr("...") e trf!("...") fora de módulos de teste, e listas constantes exibidas com tr()
"""
import json, re, sys, glob, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LANGS = ["en", "es"]
PO = lambda l: os.path.join(ROOT, "resources", "i18n", l, "LC_MESSAGES", "workout.po")
STR = r'"((?:[^"\\]|\\.)*)"'
# Listas constantes cujos itens são exibidos com tr() em tempo de execução.
CONST_LISTS = ["TEXT_SIZES", "DENSITIES", "ANIM_MODES", "ANIM_SPEEDS", "TONE_NAMES", "WEEKDAY_SHORT",
               "WEEKDAY_LONG", "MONTHS", "SET_KINDS", "GROUP_KINDS", "FAQ"]

def unescape(s):
    out = []; i = 0
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            n = s[i + 1]; out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(n, "\\" + n)); i += 2
        else:
            out.append(c); i += 1
    return "".join(out)

def po_escape(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")

def extract():
    ids = []
    def add(s):
        s = unescape(s)
        if s and s not in ids:
            ids.append(s)
    for f in sorted(glob.glob(os.path.join(ROOT, "ui", "**", "*.slint"), recursive=True)):
        for m in re.finditer(r'@tr\(\s*' + STR, open(f, encoding="utf-8").read()):
            add(m.group(1))
    for f in sorted(glob.glob(os.path.join(ROOT, "src", "**", "*.rs"), recursive=True)):
        text = open(f, encoding="utf-8").read()
        text = re.split(r'\n#\[cfg\(test\)\]', text)[0]
        for m in re.finditer(r'(?<![\w.])(?:tr\(|trf!\(\s*)' + STR, text):
            add(m.group(1))
        for name in CONST_LISTS:
            for m in re.finditer(r'(?:const|static) ' + name + r'\b[^=]*=\s*&?\[(.*?)\];', text, re.S):
                for s in re.finditer(STR, m.group(1)):
                    add(s.group(1))
        if f.endswith("templates.rs"):
            for m in re.finditer(r'\b(?:name|description):\s*' + STR, text):
                add(m.group(1))
        if f.endswith("shortcuts.rs"):
            for m in re.finditer(r'\("[a-z_]+",\s*' + STR, text):
                add(m.group(1))
    return ids

def read_po(path):
    d = {}
    if not os.path.exists(path):
        return d
    cur = None; mid = []; mstr = []; state = None
    def flush():
        if mid is not None and state == "str":
            k = unescape("".join(mid)); v = unescape("".join(mstr))
            if k: d[k] = v
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if line.startswith("msgid "):
            flush(); mid = [re.match(r'msgid ' + STR, line).group(1)]; mstr = []; state = "id"
        elif line.startswith("msgstr "):
            mstr = [re.match(r'msgstr ' + STR, line).group(1)]; state = "str"
        elif line.startswith('"'):
            (mid if state == "id" else mstr).append(re.match(STR, line).group(1))
    flush()
    return d

def write_po(path, lang, ids, trans):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write('# Traduções do EMPETO. Texto-fonte: português (pt-BR). Gerado por scripts/i18n.py.\n')
        f.write('msgid ""\nmsgstr ""\n"Content-Type: text/plain; charset=UTF-8\\n"\n"Language: %s\\n"\n\n' % lang)
        for k in ids:
            f.write('msgid "%s"\nmsgstr "%s"\n\n' % (po_escape(k), po_escape(trans.get(k, ""))))

def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    ids = extract()
    cats = {l: read_po(PO(l)) for l in LANGS}
    if cmd == "merge":
        new = json.load(open(sys.argv[2], encoding="utf-8"))
        for k, v in new.items():
            for l in LANGS:
                if v.get(l):
                    cats[l][k] = v[l]
        cmd = "update"
    if cmd == "update":
        for l in LANGS:
            write_po(PO(l), l, ids, cats[l])
    missing = {l: [k for k in ids if not cats[l].get(k)] for l in LANGS}
    print(f"{len(ids)} frases; faltando: " + ", ".join(f"{l}={len(missing[l])}" for l in LANGS))
    if cmd == "missing":
        json.dump({k: {"en": "", "es": ""} for k in ids if any(k in missing[l] for l in LANGS)},
                  sys.stdout, ensure_ascii=False, indent=1)
    if cmd == "check" and any(missing.values()):
        for l in LANGS:
            for k in missing[l][:20]:
                print(f"  [{l}] {k!r}")
        sys.exit(1)

if __name__ == "__main__":
    main()
