"""Round-trip native exports through the pinned reference terminal CLI.

Pass a read-only Spider-Paint checkout. Node is only used to run the original
renderer in this optional compatibility test.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

PIN = "6f29a123f92e50aaa044d0f6c272b387ad0e697f"
parser = argparse.ArgumentParser()
parser.add_argument("reference", type=Path)
parser.add_argument("--binary", type=Path, default=Path("target/release/spi3derr"))
args = parser.parse_args()
ref = args.reference.resolve()
binary = args.binary.resolve()
revision = subprocess.check_output(["git", "-C", ref, "rev-parse", "HEAD"], text=True).strip()
if revision != PIN:
    raise SystemExit(f"Expected reference {PIN}, got {revision}")

frame = ["--once", "--cols", "120", "--rows", "40"]
models = Path(__file__).resolve().parents[1] / "models"
with tempfile.TemporaryDirectory() as temp:
    folder = Path(temp)
    draft, saved, script = (folder / n for n in ("copy.3draft", "copy.scene.json", "commands"))
    for original in sorted(models.glob("*.scene.json")):
        for source, dest in [(original, draft), (draft, saved)]:
            script.write_text(f"/dl {dest}\n")
            subprocess.run([binary, source, "--script", script, *frame], check=True,
                           stdout=subprocess.DEVNULL)
            assert json.loads(source.read_bytes()) == json.loads(dest.read_bytes()), original
        for renderer in ([binary], ["node", ref / "bin/spider-term.mjs"]):
            before = subprocess.check_output([*renderer, original, *frame])
            after = subprocess.check_output([*renderer, draft, *frame])
            assert before == after, (original, renderer)
        print(original.name, flush=True)
