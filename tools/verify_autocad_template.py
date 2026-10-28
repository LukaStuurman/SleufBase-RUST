"""Read-only DXF acceptance with an installed, licensed AutoCAD Core Console.

No desktop automation. Import into an isolated scratch drawing, AUDIT, save a
temporary DWG. Optionally reopen/AUDIT that DWG. The input DXF is never overwritten.
AutoCAD can return exit code zero after discarding invalid DXF: inspect its
diagnostics, entity marker and saved DWG as well as the process exit status.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess


def run_console(console, folder, name, commands, drawing=None):
    script = folder / f"{name}.scr"
    script.write_text(commands + '\n(command "_.QUIT" "_Y")\n', encoding="utf-8")
    command = [str(console), "/s", str(script), "/l", "en-US", "/isolate",
               "SleufBaseDxfAcceptance", str(folder / "profile")]
    if drawing is not None:
        command += ["/i", str(drawing)]
    with (folder / f"{name}.log").open("wb") as output:
        process = subprocess.Popen(command, cwd=folder, stdin=subprocess.DEVNULL,
            stdout=output, stderr=subprocess.STDOUT,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        try:
            process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            process.kill()  # Only the console process created by this check.
            process.wait()
            raise RuntimeError("AutoCAD acceptance timed out") from None
    data = (folder / f"{name}.log").read_bytes()
    text = data.decode("utf-16-le" if data.count(b"\0") > len(data) // 5 else "utf-8", errors="replace")
    text = text.replace("\r", "")
    (folder / f"{name}.txt").write_text(text, encoding="utf-8")
    if process.returncode != 0:
        raise RuntimeError(f"AutoCAD exited with {process.returncode}")
    return inspect_output(text)


def inspect_output(text):
    if re.search(r"invalid or incomplete DXF|DXF read error|Error in .+ Table|drawing discarded", text, re.I):
        raise RuntimeError("AutoCAD rejected the DXF; inspect the console log")
    entities = re.findall(r"SLEUFBASE_ENTITIES=(\d+)", text)
    audits = re.findall(r"Total errors found\s+(\d+)\s+fixed\s+(\d+)", text)
    if not entities or int(entities[-1]) == 0 or not audits or any(pair != ("0", "0") for pair in audits):
        raise RuntimeError("AutoCAD did not confirm a populated drawing with a clean audit")
    return {"entities": int(entities[-1]), "audits": len(audits)}


CHECK = '(command "_.AUDIT" "_Y")\n(princ (strcat "SLEUFBASE_ENTITIES=" (itoa (sslength (ssget "_X")))))\n'


def verify(console, drawing, folder, reopen=False):
    folder.mkdir(parents=True, exist_ok=False)
    saved = folder / "roundtrip.dwg"
    imported = run_console(console, folder, "import",
        '(setvar "FILEDIA" 0)\n'
        f'(command "_.DXFIN" "{drawing.as_posix()}")\n' + CHECK +
        f'(command "_.QSAVE" "{saved.as_posix()}")\n')
    if not saved.is_file() or saved.stat().st_size < 100:
        raise RuntimeError("AutoCAD did not save the imported drawing")
    reopened = None
    if reopen:
        reopened = run_console(console, folder, "reopen", CHECK, drawing=saved)
        if imported["entities"] != reopened["entities"]:
            raise RuntimeError("Entity count changed across the AutoCAD DWG roundtrip")
    return {"input": str(drawing), "import": imported, "reopen": reopened,
            "saved_dwg_bytes": saved.stat().st_size}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--console", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New directory for evidence")
    parser.add_argument("--reopen", action="store_true", help="Also test opening the temporary DWG in another console process")
    parser.add_argument("drawing", type=Path)
    args = parser.parse_args()
    result = verify(args.console.resolve(strict=True), args.drawing.resolve(strict=True), args.output.resolve(), args.reopen)
    (args.output / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result))
