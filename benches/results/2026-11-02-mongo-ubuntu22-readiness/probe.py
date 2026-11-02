from pathlib import Path
import json,platform,re,subprocess,time,uuid
p=Path(__file__).resolve().parent
IMAGE='mongo@sha256:b9924c330a8bbc5800fe9c69f04a84e221eca7b28fd049a0354147d9908fa142'

def kernel_affected(release):
    match=re.match(r'^(\d+)\.(\d+)\.(\d+)(?:[-+.]|$)',release)
    if not match or 'rc'in release:raise ValueError('Unrecognized or prerelease kernel')
    return (6,19,0)<=tuple(map(int,match.groups()))<(7,0,14)

def main():
    status=dict(image=IMAGE,host=platform.platform(),kernel_release=platform.release(),benchmark_timings=0,ready=False,attempts=[],cleanup_verified=False)
    cid=None;owner='lin-mongo-ubuntu22-'+uuid.uuid4().hex
    try:
        status['affected_kernel']=kernel_affected(platform.release())
        assert platform.system()=='Linux'and platform.machine()=='x86_64'
        assert not status['affected_kernel'],'Affected kernel: do not start MongoDB'
        status['docker_version']=json.loads(subprocess.check_output(['docker','version','--format','{{json .}}'],text=True))
        (p/'cpuinfo.txt').write_text(Path('/proc/cpuinfo').read_text())
        from pymongo import MongoClient
        subprocess.run(['docker','pull',IMAGE],check=True)
        cid=subprocess.check_output(['docker','run','-d','--name',owner,'--label','lin.benchmark.owner='+owner,'-p','127.0.0.1:27029:27017','--tmpfs','/data/db:rw,size=2g',IMAGE,'--bind_ip_all','--wiredTigerCacheSizeGB','0.25'],text=True).strip()
        for i in range(30):
            client=MongoClient('mongodb://127.0.0.1:27029',serverSelectionTimeoutMS=1000)
            try:
                client.admin.command('ping')
                db=client['lin_readiness_'+owner.replace('-','_')]
                try:
                    result=db.docs.insert_one({'_id':'probe','text':'ready','n':17})
                    assert result.inserted_id=='probe'
                    assert db.docs.find_one({'_id':'probe'})=={'_id':'probe','text':'ready','n':17}
                    assert db.docs.delete_one({'_id':'probe'}).deleted_count==1
                    assert db.docs.count_documents({})==0
                    status['server_version']=client.admin.command('buildInfo')['version']
                    status['ready']=True;status['crud_verified']=True;break
                finally:client.drop_database(db.name)
            except Exception as e:status['attempts'].append(dict(attempt=i,error=str(e)))
            finally:client.close()
            state=json.loads(subprocess.check_output(['docker','inspect',cid],text=True))[0]['State']
            if not state['Running']:break
            time.sleep(1)
    finally:
        if cid:
            inspect=json.loads(subprocess.check_output(['docker','inspect',cid],text=True))[0]
            assert inspect['Config']['Labels']['lin.benchmark.owner']==owner
            (p/'container-inspect.json').write_text(json.dumps(inspect,indent=2)+'\n')
            log=subprocess.run(['docker','logs',cid],capture_output=True,text=True)
            (p/'mongo.log').write_text(log.stdout+log.stderr)
            subprocess.run(['docker','stop',cid],check=True,stdout=subprocess.DEVNULL)
            subprocess.run(['docker','rm',cid],check=True,stdout=subprocess.DEVNULL)
            status['cleanup_verified']=subprocess.run(['docker','inspect',cid],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==1
        (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
    assert status['ready']and status['crud_verified']and status['cleanup_verified']
    print(json.dumps(dict(ready=True,kernel=status['kernel_release'],server_version=status['server_version'],crud_verified=True,cleanup_verified=True,benchmark_timings=0)))

if __name__=='__main__':main()
