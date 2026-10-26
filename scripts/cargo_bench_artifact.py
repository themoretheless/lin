"""Locate one Cargo benchmark executable in plain or colored human-readable output."""
import re


def bench_executable(log: str, name: str) -> str:
    plain = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    paths = re.findall(
        rf'^\s*Executable\s+benches/{re.escape(name)}\.rs\s+\(([^)\r\n]+)\)\s*$',
        plain, re.MULTILINE,
    )
    if len(paths) != 1:
        raise ValueError(f'Expected one {name} benchmark executable, found {len(paths)}')
    return paths[0]
