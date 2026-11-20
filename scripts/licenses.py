#!/usr/bin/env python3
"""Gera docs/LICENCAS-TERCEIROS.md a partir do `cargo metadata` (dependências Rust e suas licenças).
Uso: scripts/licenses.py"""
import collections, json, os, subprocess

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=root))
pkgs = sorted((p for p in meta["packages"] if p["source"]), key=lambda p: (p["name"], p["version"]))
by = collections.defaultdict(list)
for p in pkgs:
    by[p.get("license") or "(não declarada)"].append(f'{p["name"]} {p["version"]}')
lines = [
    "# Licenças de terceiros",
    "",
    "Gerado por `scripts/licenses.py` a partir do `Cargo.lock` (inclui dependências de todas as plataformas e de",
    "build; nem todas entram no APK). Revisar sempre que dependências forem atualizadas.",
    "",
    f"Total: {len(pkgs)} pacotes.",
    "",
    "| Licença | Pacotes |",
    "|---|---|",
]
for lic, names in sorted(by.items(), key=lambda kv: -len(kv[1])):
    lines.append(f"| {lic} | {len(names)} |")
lines.append("")
for lic, names in sorted(by.items(), key=lambda kv: -len(kv[1])):
    lines += [f"## {lic}", "", ", ".join(names), ""]
open(os.path.join(root, "docs", "LICENCAS-TERCEIROS.md"), "w", encoding="utf-8").write("\n".join(lines))
print(f"{len(pkgs)} pacotes, {len(by)} licenças")
