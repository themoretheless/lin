from pathlib import Path
import subprocess, re, shutil, json, hashlib
root = Path(__file__).resolve().parent
variants = {name: (root / "variants" / f"{name}-index.rs").read_bytes() for name in ["vec", "inline"]}
bins = Path("/tmp/lin-inline-index-prebuilt")
bins.mkdir(exist_ok=True)
manifest = {}
try:
    for name, data in variants.items():
        Path("src/index.rs").write_bytes(data)
        cmd = ["cargo", "bench", "--bench", "compare", "--offline", "--", "--filter", "__build_only__", "--exact", "--output", str(root / f"build-{name}")]
        result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (root / f"build-{name}.log").write_text(result.stdout)
        exe = re.search(r"Running benches/compare.rs \(([^)]+)\)", result.stdout).group(1)
        target = bins / name
        shutil.copy2(exe, target)
        manifest[name] = {"sha256": hashlib.sha256(target.read_bytes()).hexdigest(), "source_sha256": hashlib.sha256(data).hexdigest()}
        print("built", name, flush=True)
    (root / "prebuilt-sha256.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for pair in range(1, 7):
        for name in (["vec", "inline"] if pair % 2 else ["inline", "vec"]):
            with (root / f"rapid-{pair}-{name}.log").open("w") as log:
                subprocess.run([str(bins / name), "--filter", "compare/insert_phase*", "--glob", "--samples", "12", "--max-iterations", "1", "--warmup-ms", "0", "--output", str(root / f"rapid-{pair}-{name}")], stdout=log, stderr=subprocess.STDOUT, check=True)
            print("rapid", pair, name, flush=True)
finally:
    Path("src/index.rs").write_bytes(variants["inline"])
    print("candidate restored", flush=True)
