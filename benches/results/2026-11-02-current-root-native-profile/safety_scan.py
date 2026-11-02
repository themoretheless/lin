"""Count secret-like bytes without printing or returning matched values."""
from pathlib import Path
import re
PATTERNS={
 'github_token_value':rb'gh[sopru]_[A-Za-z0-9_]{30,}',
 'credential_environment_assignment':rb'(?:GH_TOKEN|GITHUB_TOKEN|ACTIONS_RUNTIME_TOKEN|ACTIONS_ID_TOKEN_REQUEST_TOKEN|MSSQL_SA_PASSWORD)=[^\x00\r\n ]{8,}',
 'jwt_value':rb'eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}',
}
def scan_directory(directory):
 return {str(f.relative_to(directory)):{name:len(re.findall(pattern,f.read_bytes()))for name,pattern in PATTERNS.items()}for f in sorted(Path(directory).rglob('*'))if f.is_file()}
