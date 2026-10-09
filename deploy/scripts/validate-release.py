#!/usr/bin/env python3
"""解包前拒绝越界/链接/设备文件，检查发布目录不引用外部路径。"""
import argparse
import pathlib
import re
import tarfile


def archive(path):
    tops=set()
    with tarfile.open(path, 'r:gz') as bundle:
        for member in bundle:
            name=pathlib.PurePosixPath(member.name)
            if name.is_absolute() or '..' in name.parts or not name.parts:
                raise ValueError('发布包含越界路径')
            if not (member.isfile() or member.isdir()):
                raise ValueError('发布包不能包含链接、设备或其它特殊文件')
            tops.add(name.parts[0])
    if len(tops)!=1: raise ValueError('发布包必须只有一个根目录')


def directory(path):
    root=pathlib.Path(path).resolve()
    for file in root.rglob('*'):
        if file.is_symlink() or not (file.is_file() or file.is_dir()):
            raise ValueError('发布目录不能含符号链接或特殊文件')
    checksum=root/'SHA256SUMS'
    if checksum.exists():
        for line in checksum.read_text().splitlines():
            match=re.fullmatch(r'[0-9a-fA-F]{64} [ *](.+)',line)
            if not match: raise ValueError('校验清单格式无效')
            name=pathlib.PurePosixPath(match[1])
            if name.is_absolute() or '..' in name.parts:
                raise ValueError('校验清单不能引用发布目录之外的文件')
            if not (root/name).is_file(): raise ValueError('校验清单引用不存在的文件')

if __name__=='__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('--archive'); parser.add_argument('--directory')
    args=parser.parse_args()
    try:
        if args.archive: archive(args.archive)
        elif args.directory: directory(args.directory)
        else: parser.error('需要 archive 或 directory')
    except (ValueError, OSError, tarfile.TarError) as error:
        raise SystemExit(str(error))
