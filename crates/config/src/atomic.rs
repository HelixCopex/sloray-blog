//! 以私有权限创建临时文件，再原子发布；从不先创建可被其他用户读到的密钥。

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn stage(path: &Path, bytes: &[u8]) -> anyhow::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("文件路径缺少名称"))?;
    for _ in 0..64 {
        let tmp = parent.join(format!(
            ".{}.{}-{}.tmp",
            name.to_string_lossy(),
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&tmp) {
            Ok(mut file) => {
                if let Err(err) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(err.into());
                }
                return Ok(tmp);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    anyhow::bail!("无法创建配置临时文件")
}

fn sync_parent(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    Ok(())
}

pub fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = stage(path, bytes)?;
    let result = std::fs::rename(&tmp, path);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    sync_parent(path)
}

/// 多进程首次创建：hard_link 的目标存在检查与发布是同一个原子操作。
/// 返回 false 表示另一个进程先发布成功，调用者应读取它的值。
pub fn create_private(path: &Path, bytes: &[u8]) -> anyhow::Result<bool> {
    let tmp = stage(path, bytes)?;
    let result = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match result {
        Ok(()) => {
            sync_parent(path)?;
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(err) => Err(err.into()),
    }
}
