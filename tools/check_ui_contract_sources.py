"""Check the original UI oracle's source identities without importing the app."""
import argparse
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original-root", required=True, type=Path)
    args = parser.parse_args()
    contract = Path(__file__).resolve().parents[1] / "rust/tests/fixtures/ui/original-shell.json"
    evidence = json.loads(contract.read_text(encoding="utf-8"))["evidence"]
    root = args.original_root.resolve(strict=True)
    failures = []
    for item in evidence:
        source = (root / item["path"]).resolve()
        if not source.is_relative_to(root):
            raise ValueError("Evidence path leaves the original source directory")
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if digest != item["sha256"]:
            failures.append(item["path"])
    if failures:
        raise SystemExit("UI reference sources changed: " + ", ".join(failures))
    print(f"{len(evidence)} original UI source hashes match; no application code executed.")


if __name__ == "__main__":
    main()
