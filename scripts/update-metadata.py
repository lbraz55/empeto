#!/usr/bin/env python3
"""Gera dist/empeto-update.json para a Release (documento de evolução v1.1, 7.1).

Uso (no CI, depois de gerar o APK):
  scripts/update-metadata.py <versão> <version_code> <owner/repo>

Campos: versão, código Android, data, notas (seção da versão no CHANGELOG) e, para cada APK em dist/,
nome, URL de download da Release, tamanho, SHA-256 e ABIs incluídas.
"""
import datetime, glob, hashlib, json, os, re, sys, zipfile

version, code, repo = sys.argv[1], int(sys.argv[2]), sys.argv[3]
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
notes = ""
try:
    text = open(os.path.join(root, "CHANGELOG.md"), encoding="utf-8").read()
    m = re.search(r"^## " + re.escape(version) + r"\b.*?\n(.*?)(?=^## |\Z)", text, re.S | re.M)
    if m:
        notes = m.group(1).strip()
except OSError:
    pass
apks = []
for path in sorted(glob.glob(os.path.join(root, "dist", "*.apk"))):
    name = os.path.basename(path)
    h = hashlib.sha256(open(path, "rb").read()).hexdigest()
    with zipfile.ZipFile(path) as z:
        abis = sorted({n.split("/")[1] for n in z.namelist() if n.startswith("lib/") and n.count("/") >= 2})
    apks.append({
        "name": name,
        "url": f"https://github.com/{repo}/releases/download/v{version}/{name}",
        "size": os.path.getsize(path),
        "sha256": h,
        "abis": abis,
    })
if not apks:
    sys.exit("nenhum APK em dist/")
meta = {
    "version": version,
    "version_code": code,
    "published_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "notes": notes,
    "apks": apks,
}
out = os.path.join(root, "dist", "empeto-update.json")
json.dump(meta, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(f"{out}: {len(apks)} APK(s), {version} ({code})")
