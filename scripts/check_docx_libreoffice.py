#!/usr/bin/env python3
"""S4 check: a DOCX produced by `morph convert --to docx` opens in LibreOffice.

Builds a 3-page PDF with the CLI (images → PDF needs no PDF fixture), converts it to
DOCX with morph, then asks headless LibreOffice to convert the DOCX back to PDF.
"""
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
exe = ROOT / "target" / "debug" / ("morph.exe" if sys.platform == "win32" else "morph")
soffice = shutil.which("soffice") or shutil.which("libreoffice")
if not soffice:
    print("LibreOffice not installed"); sys.exit(1)
subprocess.run(["cargo", "build", "-p", "morph-cli"], cwd=ROOT, check=True)

with tempfile.TemporaryDirectory() as d:
    d = pathlib.Path(d)
    svg = d / "page.svg"
    svg.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="595" height="842">'
                   '<rect width="595" height="842" fill="white"/><circle cx="300" cy="400" r="120" fill="#4FE3FF"/></svg>')
    subprocess.run([exe, "convert", "--to", "png", str(svg)], check=True)
    subprocess.run([exe, "convert", "--to", "pdf", str(d / "page.png")], check=True)
    subprocess.run([exe, "convert", "--to", "docx", str(d / "page.pdf")], check=True)
    docx = d / "page.docx"
    out = d / "lo"
    r = subprocess.run([soffice, "--headless", "--convert-to", "pdf", "--outdir", str(out), str(docx)],
                       capture_output=True, text=True, timeout=180)
    print(r.stdout, r.stderr)
    if not (out / "page.pdf").exists():
        print("LibreOffice could not open the DOCX"); sys.exit(1)
    print("DOCX opens in LibreOffice")
