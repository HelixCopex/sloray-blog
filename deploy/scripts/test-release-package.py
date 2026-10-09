#!/usr/bin/env python3
import importlib.util
import io
import pathlib
import tarfile
import tempfile
spec=importlib.util.spec_from_file_location('validate',pathlib.Path(__file__).with_name('validate-release.py'))
validate=importlib.util.module_from_spec(spec); spec.loader.exec_module(validate)
with tempfile.TemporaryDirectory() as tmp:
    root=pathlib.Path(tmp)
    for name,kind,good in [('sloray/VERSION',tarfile.REGTYPE,True),('../escape',tarfile.REGTYPE,False),('/tmp/escape',tarfile.REGTYPE,False),('sloray/link',tarfile.SYMTYPE,False),('sloray/hard',tarfile.LNKTYPE,False),('sloray/device',tarfile.CHRTYPE,False)]:
        archive=root/'release.tar.gz'
        with tarfile.open(archive,'w:gz') as out:
            member=tarfile.TarInfo(name); member.type=kind; member.linkname='/etc/passwd'
            member.size=1 if kind==tarfile.REGTYPE else 0
            out.addfile(member,io.BytesIO(b'x') if member.size else None)
        try: validate.archive(archive)
        except ValueError: assert not good,name
        else: assert good,name
    (root/'link').symlink_to('/etc/passwd')
    try: validate.directory(root); raise AssertionError('symlink accepted')
    except ValueError: pass
    (root/'link').unlink()
    (root/'SHA256SUMS').write_text('0'*64+'  ../../etc/passwd\n')
    try: validate.directory(root); raise AssertionError('checksum escape accepted')
    except ValueError: pass
    print('PASS 发布包正常路径可接受；拒绝绝对/穿越路径、符号/硬链接、设备与校验清单越界')
