//! systemd 单元的嵌入与安装。
//!
//! `cargo install` 只能装可执行文件，没法把单元文件放进 `/etc/systemd/system`。
//! 折中做法：把单元**模板编进二进制**，由一个子命令渲染 + 落盘 + 启用。
//!
//! ```text
//! cargo install --path . --root /usr/local
//! sudo /usr/local/bin/sloray systemd install --shared /srv/sloray/shared
//! ```
//!
//! 模板里只有三处占位符，渲染时用真实路径替换：
//!
//! * `@SLORAY_BIN@` —— 二进制自身的路径（用 `current_exe` 取，不靠猜）
//! * `@SLORAY_SHARED@` —— 运行时配置目录（同时是唯一可写目录）
//! * `@WEB_ENTRY@` / `@WEB_WORKDIR@` —— Astro 的入口与工作目录
//!
//! 用占位符而不是在 Rust 与 shell 里各写一份单元，是为了**只有一份事实来源**：
//! `deploy/scripts/activate-release.sh` 渲染的是同一批模板。

use std::path::{Path, PathBuf};

/// API 单元模板（同时被 install-release 流程使用）。
pub const API_UNIT_TEMPLATE: &str = include_str!("../../../deploy/systemd/sloray@.service.in");

/// Web 单元模板。
pub const WEB_UNIT_TEMPLATE: &str = include_str!("../../../deploy/systemd/sloray-web.service.in");
pub const WORKER_UNIT_TEMPLATE: &str =
    include_str!("../../../deploy/systemd/sloray-worker.service.in");
pub const WORKER_TIMER_TEMPLATE: &str =
    include_str!("../../../deploy/systemd/sloray-worker.timer.in");

pub fn render_worker_unit(bin: &Path, shared: &Path) -> String {
    WORKER_UNIT_TEMPLATE
        .replace("@SLORAY_BIN@", &bin.display().to_string())
        .replace("@SLORAY_SHARED@", &shared.display().to_string())
}

/// 版本目录部署时 Astro 入口的相对位置。
pub const DEFAULT_WEB_ENTRY: &str = "/srv/sloray/current/web/dist/server/entry.mjs";
/// 版本目录部署时的共享目录。
pub const DEFAULT_SHARED_DIR: &str = "/srv/sloray/shared";
/// 渲染后的单元默认落盘位置。
pub const DEFAULT_UNIT_DIR: &str = "/etc/systemd/system";

/// 渲染 API 单元。
pub fn render_api_unit(bin: &Path, shared_dir: &Path) -> String {
    API_UNIT_TEMPLATE
        .replace("@SLORAY_BIN@", &bin.display().to_string())
        .replace("@SLORAY_SHARED@", &shared_dir.display().to_string())
}

/// 渲染 Web 单元。
pub fn render_web_unit(entry: &Path) -> String {
    let workdir = entry.parent().unwrap_or_else(|| Path::new("/"));
    WEB_UNIT_TEMPLATE
        .replace("@WEB_ENTRY@", &entry.display().to_string())
        .replace("@WEB_WORKDIR@", &workdir.display().to_string())
}

/// 渲染后是否还残留未替换的占位符。
///
/// 这个检查很重要：漏掉一个占位符的结果是 systemd 里出现一个
/// `@SOMETHING@` 字面量，报错信息很难指向真正的原因。
pub fn assert_fully_rendered(text: &str) -> anyhow::Result<()> {
    if let Some(idx) = text.find('@')
        && let Some(end) = text[idx + 1..].find('@')
    {
        let placeholder = &text[idx..idx + end + 2];
        // 单元文件里合法出现 @ 的地方只有 systemd 的转义（例如 %%），
        // 这里保守处理：只把形如 @NAME@ 的当成残留占位符。
        if placeholder.len() > 2
            && placeholder
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_uppercase())
        {
            anyhow::bail!("单元模板里有未替换的占位符：{placeholder}");
        }
    }
    Ok(())
}

/// 把单元写到目标目录。返回实际写入的文件名。
pub fn write_units(
    dir: &Path,
    api_unit: &str,
    web_unit: &str,
    worker_unit: &str,
) -> anyhow::Result<Vec<PathBuf>> {
    assert_fully_rendered(api_unit)?;
    assert_fully_rendered(web_unit)?;
    assert_fully_rendered(worker_unit)?;

    std::fs::create_dir_all(dir)
        .map_err(|e| anyhow::anyhow!("创建 {} 失败：{e}", dir.display()))?;

    let mut written = Vec::new();
    for (name, content) in [
        ("sloray@.service", api_unit),
        ("sloray-web.service", web_unit),
        ("sloray-worker.service", worker_unit),
        ("sloray-worker.timer", WORKER_TIMER_TEMPLATE),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, content)
            .map_err(|e| anyhow::anyhow!("写入 {} 失败：{e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// 当前可执行文件的绝对路径。
///
/// 用 `current_exe` 而不是猜路径：`cargo install` 可能装到
/// `~/.cargo/bin`、`/usr/local/bin`，也可能带 `--root` 到任意位置。
pub fn current_binary() -> anyhow::Result<PathBuf> {
    std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("无法确定自身路径（current_exe 失败）：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_unit_renders_all_placeholders() {
        let rendered = render_api_unit(
            Path::new("/usr/local/bin/sloray"),
            Path::new("/srv/sloray/shared"),
        );
        assert!(rendered.contains("ExecStart=/usr/local/bin/sloray serve --bind 127.0.0.1:%i"));
        assert!(rendered.contains("Environment=SLORAY_CONFIG_FILE=/srv/sloray/shared/config.json"));
        assert!(rendered.contains("ReadWritePaths=/srv/sloray/shared"));
        assert!(rendered.contains("WorkingDirectory=/srv/sloray/shared"));
        assert_fully_rendered(&rendered).unwrap();
    }

    #[test]
    fn web_unit_renders_all_placeholders() {
        let rendered = render_web_unit(Path::new("/srv/sloray/current/web/dist/server/entry.mjs"));
        assert!(
            rendered
                .contains("ExecStart=/usr/bin/node /srv/sloray/current/web/dist/server/entry.mjs")
        );
        assert!(rendered.contains("WorkingDirectory=/srv/sloray/current/web/dist/server"));
        assert_fully_rendered(&rendered).unwrap();
    }

    #[test]
    fn leftover_placeholder_is_detected() {
        // 这是最容易悄悄发生的一类错误：改了模板名却没改渲染代码。
        let broken = "[Service]\nExecStart=@SLORAY_BIN@ serve\n";
        let err = assert_fully_rendered(broken).unwrap_err().to_string();
        assert!(
            err.contains("@SLORAY_BIN@"),
            "错误信息应当指出是哪个占位符：{err}"
        );
    }

    #[test]
    fn templates_are_embedded_at_compile_time() {
        // 如果模板文件被删/改名，这里会直接编译失败 —— 这正是想要的效果。
        assert!(API_UNIT_TEMPLATE.contains("sloray-blog API"));
        assert!(WEB_UNIT_TEMPLATE.contains("Astro SSR"));
    }

    #[test]
    fn units_keeps_the_hardening_intact() {
        let rendered = render_api_unit(Path::new("/bin/sloray"), Path::new("/srv/sloray/shared"));
        // 引入 Web 配置界面后唯一的放宽点就是这一处可写路径。
        assert!(rendered.contains("ProtectSystem=strict"));
        assert!(rendered.contains("NoNewPrivileges=true"));
        assert_eq!(
            rendered.matches("ReadWritePaths=").count(),
            1,
            "可写路径只应有一处，不要顺手扩大"
        );
    }
}
