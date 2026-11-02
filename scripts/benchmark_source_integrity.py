"""Content identities for peer benchmark inputs; generated reports are excluded."""
from pathlib import Path
import hashlib


def snapshot(root: Path, binary: Path) -> dict:
    root = root.resolve()
    files = set()
    for directory, suffixes in [('src', {'.rs'}), ('examples', {'.rs'}),
                                 ('benches', {'.rs'}), ('scripts', {'.py', '.sh'}),
                                 ('lin-derive/src', {'.rs'})]:
        base = root / directory
        if directory == 'benches':
            candidates = base.glob('*.rs')
        else:
            candidates = base.rglob('*')
        files.update(f for f in candidates if f.is_file() and f.suffix in suffixes)
    for name in ['Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain', 'rust-toolchain.toml', 'lin-derive/Cargo.toml']:
        f = root / name
        if f.is_file():
            files.add(f)
    config = root / '.cargo'
    if config.exists():
        files.update(f for f in config.rglob('*') if f.is_file())
    if not (root / 'Cargo.toml').is_file() or not (root / 'src').is_dir():
        raise ValueError('runtime/build roots missing')
    if not binary.is_absolute():
        binary = root / binary
    # Missing/unreadable binary is an integrity failure, never a clean snapshot.
    binary_bytes = binary.read_bytes()
    digest = lambda data: hashlib.sha256(data).hexdigest()
    return {'files_sha256': {str(f.relative_to(root)): digest(f.read_bytes()) for f in sorted(files)},
            'binary': {'path': str(binary.resolve()), 'bytes': len(binary_bytes), 'sha256': digest(binary_bytes)}}
