from pathlib import Path
import runpy
scope=runpy.run_path(str(Path(__file__).with_name('audit-traces.py')))['scope']
begin='42 write(2<pipe:[123]>, "LIN_IO_BEGIN\\n", 13) = 13\n'
end='42 write(2<pipe:[123]>, "LIN_IO_END\\n", 11) = 11\n'
body='42 pwrite64(7</tmp/db-wal>, "data", 4, 24) = 4\n42 fdatasync(7</tmp/db-wal>) = 0\n'
extra='42 openat(AT_FDCWD, "/tmp/file", O_RDWR) = 8</tmp/file>\n42 fcntl(8</tmp/file>, F_GETFL) = 0x8002 (flags O_RDWR|O_LARGEFILE)\n'
assert [r['result']for r in scope(begin+extra+end)]==[8,32770]
rows=scope(begin+body+end);assert len(rows)==2 and rows[0]['result']==4 and rows[1]['path']=='/tmp/db-wal'
for invalid in [body+end,begin+body,begin+begin+body+end,end+body+begin,begin+body.replace('= 4','= -1 EIO (Input/output error)')+end,begin+'42 fdatasync(7</tmp/db-wal> <unfinished ...>\n'+end]:
 try:scope(invalid)
 except AssertionError:pass
 else:raise AssertionError('corrupt trace accepted')
print('PASS three positives and six invalid scope tests')
