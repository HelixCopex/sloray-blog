use clap::{Parser, Subcommand};
use sloray_setup::systemd;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sloray",
    version,
    about = "sloray-blog 服务：Rust API + 迁移 + systemd 安装"
)]
pub(crate) struct Cli {
    /// 运行时配置文件路径（默认 /srv/sloray/shared/config.json）。
    /// 也可以用环境变量 `SLORAY_CONFIG_FILE` 指定。
    #[arg(long, env = "SLORAY_CONFIG_FILE", global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// 后台任务：定时发布与过期状态清理；--once 可用于调度器。
    Worker {
        #[arg(long)]
        once: bool,
    },
    /// 从浏览去重记录与点赞重建计数。
    RebuildStats,
    /// 从文章原文全量重建中文全文检索索引。
    RebuildSearch,
    /// 重新投递所有未删除媒体的派生图任务。
    RebuildMedia,
    /// 列出超过 24h 的孤儿原图；--apply 才执行对象删除。
    MediaGc {
        #[arg(long)]
        apply: bool,
    },
    /// 输出 OpenAPI 契约，无需数据库或运行时配置。
    Openapi,
    /// 启动 HTTP 服务。
    ///
    /// 未配置数据库时**照常启动**并进入 setup 模式，
    /// 打开 `/setup` 用一次性 token 完成初始化。
    Serve {
        /// 显式监听地址，优先于配置文件与 EnvironmentFile（systemd 实例使用）。
        #[arg(long)]
        bind: Option<std::net::SocketAddr>,
    },

    /// 数据库迁移。
    Migrate {
        #[command(subcommand)]
        action: MigrateAction,
    },

    /// 渲染/安装 systemd 单元（`cargo install` 场景下的开机自启）。
    Systemd {
        #[command(subcommand)]
        action: SystemdAction,
    },

    /// 打印当前 setup token；`--rotate` 重新生成一个。
    SetupToken {
        /// 生成新 token（旧的立即失效）
        #[arg(long)]
        rotate: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MigrateAction {
    /// 应用待执行迁移（带咨询锁，可安全并发调用）
    Up {
        /// 最多应用几步（默认全部）
        steps: Option<u32>,
    },
    /// 回滚迁移（默认 1 步）
    Down { steps: Option<u32> },
    /// 查看迁移状态
    Status,
    /// 删掉所有表再重建 —— **仅用于本地/测试**
    Fresh,
}

#[derive(Subcommand)]
pub(crate) enum SystemdAction {
    /// 只渲染并打印到 stdout，不写任何文件（先看看要装什么）
    Print {
        /// 运行时共享目录（配置与 setup token 所在处）
        #[arg(long)]
        shared: Option<PathBuf>,
        /// Astro 入口文件路径
        #[arg(long)]
        web_entry: Option<PathBuf>,
    },
    /// 写入单元文件，并按需启用服务
    Install {
        /// 单元文件目录
        #[arg(long, default_value = systemd::DEFAULT_UNIT_DIR)]
        dir: PathBuf,
        #[arg(long)]
        shared: Option<PathBuf>,
        #[arg(long)]
        web_entry: Option<PathBuf>,
        /// 只写文件，不执行 systemctl daemon-reload / enable
        #[arg(long)]
        no_enable: bool,
    },
}
