"""Keep external-engine code invariant across Lin source roles."""
def validate(meta):
    peer=meta['peer_binary'];baseline=meta['binaries']['baseline'];candidate=meta['binaries']['candidate']
    assert peer['sha256']==baseline['sha256']!=candidate['sha256']
    assert peer['path']not in (baseline['path'],candidate['path'])
    assert peer['source_commit']==meta['frozen_commits']['baseline']
    assert peer['source_sha256']==baseline['source_sha256']
    assert peer['compiled_compare_sha256']==meta['compiled_compare_sha256']
    return True

def binary_for(meta,role,case):
    assert role in ('baseline','control','candidate')
    validate(meta)
    engine=case.rsplit('/',1)[-1]
    if engine=='lin':
        key='baseline'if role=='control'else role
        return key,meta['binaries'][key]
    assert engine in ('sqlite','sqlite_delete','sqlite_wal','duckdb'),engine
    return 'shared_peer',meta['peer_binary']
