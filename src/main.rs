//! `sloray` —— 整个项目唯一可执行文件。
//!
//! ```text
//! cargo install --path . --root /usr/local
//! sudo sloray systemd install          # 装单元并启用，之后开机自启
//! sudo sloray serve                    # 或交给 systemd
//! ```
//!
//! 子命令刻意合并到一个二进制里：`cargo install` 只装一次，
//! 而 systemd 单元、迁移、setup token 都与"这个二进制在哪"强相关，
//! 分成多个二进制只会让路径和版本更容易对不上。

#![forbid(unsafe_code)]

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use sea_orm::Database;
use sloray_auth as auth;
use sloray_config::Config;
use sloray_migration::{Migrator, MigratorTrait};
use sloray_setup::{self as setup, systemd};

mod cli;
use cli::{Cli, Command, MigrateAction, SystemdAction};

fn main() -> ExitCode {
    // .env 只用于本地开发；生产由 systemd 的 EnvironmentFile 提供。
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // anyhow 的 {:#} 打印完整错误链，便于定位是哪一步失败。
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn run(cli: Cli) -> anyhow::Result<()> {
    if matches!(cli.command, Command::Openapi) {
        println!(
            "{}",
            serde_json::to_string_pretty(&sloray_web::openapi::document())?
        );
        return Ok(());
    }
    // systemd 与 setup-token 只跟路径打交道，**刻意不读配置**：
    // 这两个命令要能在"配置文件坏掉/还没配"的时候工作，
    // 否则就没有办法用它去修一个坏掉的部署。
    if let Command::Systemd { action } = &cli.command {
        sloray_web::telemetry::init_default();
        return systemd_command(action);
    }

    if let Command::SetupToken { rotate } = cli.command {
        let path = cli
            .config
            .unwrap_or_else(|| PathBuf::from(sloray_config::DEFAULT_CONFIG_FILE));
        return setup_token_command(&setup::token_path(&path), rotate);
    }

    let mut config = Config::load(
        cli.config
            .clone()
            .unwrap_or_else(|| PathBuf::from(sloray_config::DEFAULT_CONFIG_FILE)),
    )?;

    // 日志初始化是幂等的（内部用 try_init），
    // 所以这里与 server::run 里各调一次都不会炸。
    sloray_web::telemetry::init(&config);

    match cli.command {
        Command::Openapi => unreachable!("上面已提前处理"),
        Command::Systemd { .. } => unreachable!("上面已提前处理"),
        Command::Serve { bind } => {
            if let Some(bind) = bind {
                config.bind_addr = bind;
            }
            sloray_web::server::run(config).await
        }
        Command::Migrate { action } => migrate_command(&config, action).await,
        Command::Worker { once } => {
            let db = sloray_store::connect_database(
                config
                    .database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("请先配置数据库"))?,
                &config,
            )
            .await?;
            loop {
                let published = sloray_content::worker::tick(&db).await?;
                let delivered = sloray_media::worker::tick(&db, &config).await?;
                tracing::info!(published, delivered, "后台任务完成");
                if once {
                    break;
                }
                tokio::select! { _=tokio::signal::ctrl_c()=>break, _=tokio::time::sleep(std::time::Duration::from_secs(5))=>{} }
            }
            Ok(())
        }
        Command::RebuildSearch => {
            let db = sloray_store::connect_database(
                config
                    .database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("请先配置数据库"))?,
                &config,
            )
            .await?;
            println!(
                "已重建 {} 篇文章的检索索引",
                sloray_content::search_index::rebuild(&db).await?
            );
            Ok(())
        }
        Command::RebuildMedia => {
            let db = sloray_store::connect_database(
                config
                    .database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("请先配置数据库"))?,
                &config,
            )
            .await?;
            println!(
                "已重新投递 {} 条媒体派生任务",
                sloray_media::worker::regenerate(&db).await?
            );
            Ok(())
        }
        Command::MediaGc { apply } => {
            let db = sloray_store::connect_database(
                config
                    .database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("请先配置数据库"))?,
                &config,
            )
            .await?;
            for key in sloray_media::gc::collect(&db, &config, apply).await? {
                println!("{} {key}", if apply { "deleted" } else { "candidate" });
            }
            Ok(())
        }
        Command::RebuildStats => {
            let db = sloray_store::connect_database(
                config
                    .database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("请先配置数据库"))?,
                &config,
            )
            .await?;
            println!(
                "已重建 {} 篇文章的计数",
                sloray_content::stats::rebuild(&db).await?
            );
            Ok(())
        }
        Command::SetupToken { .. } => unreachable!("上面已提前处理"),
    }
}

// ---------------------------------------------------------------- migrate

async fn migrate_command(config: &Config, action: MigrateAction) -> anyhow::Result<()> {
    let url = config.database_url.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "没有可用的数据库连接串。\n\
             请先在浏览器打开 /setup 完成初始配置，\
             或临时用 SLORAY_DATABASE_URL 指定。"
        )
    })?;

    // 迁移只需要一条连接，不必开连接池。
    let mut options = sea_orm::ConnectOptions::new(url);
    options
        .max_connections(1)
        .sqlx_logging(false)
        .set_application_name("sloray-migrate");
    let db = Database::connect(options).await?;

    match action {
        // up 走带咨询锁的版本：这样"部署脚本跑迁移"与"setup 向导跑迁移"
        // 即使同时发生也不会互相踩（ADR-0006）。
        MigrateAction::Up { steps } => {
            sloray_migration::migrate_with_lock(&db, sloray_migration::Operation::Up(steps))
                .await?;
            tracing::info!("迁移完成");
        }
        MigrateAction::Down { steps } => {
            let steps = steps.or(Some(1));
            tracing::warn!(?steps, "回滚迁移");
            sloray_migration::migrate_with_lock(
                &db,
                sloray_migration::Operation::Down(steps.unwrap_or(1)),
            )
            .await?;
        }
        MigrateAction::Status => {
            for migration in Migrator::get_migration_with_status_read_only(&db).await? {
                println!("{} {}", migration.name(), migration.status());
            }
        }
        MigrateAction::Fresh => {
            tracing::warn!("fresh：删除所有表并重建，仅应用于本地或测试环境");
            sloray_migration::migrate_with_lock(&db, sloray_migration::Operation::Fresh).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------- systemd

fn systemd_command(action: &SystemdAction) -> anyhow::Result<()> {
    let (shared, web_entry) = match action {
        SystemdAction::Print { shared, web_entry }
        | SystemdAction::Install {
            shared, web_entry, ..
        } => (
            shared
                .clone()
                .unwrap_or_else(|| PathBuf::from(systemd::DEFAULT_SHARED_DIR)),
            web_entry
                .clone()
                .unwrap_or_else(|| PathBuf::from(systemd::DEFAULT_WEB_ENTRY)),
        ),
    };

    // 用 current_exe 而不是猜路径：cargo install 可能装到 ~/.cargo/bin、
    // /usr/local/bin，也可能用 --root 指到任意位置。
    let bin = systemd::current_binary()?;
    for path in [&bin, &shared, &web_entry] {
        if !path.is_absolute()
            || path
                .to_string_lossy()
                .chars()
                .any(|c| c.is_whitespace() || ['%', '\"', '\\'].contains(&c))
        {
            anyhow::bail!("systemd 路径必须是绝对路径，且不能含空白、引号、反斜杠或 %");
        }
    }
    let api_unit = systemd::render_api_unit(&bin, &shared);
    let web_unit = systemd::render_web_unit(&web_entry);
    let worker_unit = systemd::render_worker_unit(&bin, &shared);

    match action {
        SystemdAction::Print { .. } => {
            println!("{api_unit}");
            println!("{web_unit}");
            println!("{worker_unit}");
            println!("{}", systemd::WORKER_TIMER_TEMPLATE);
            Ok(())
        }
        SystemdAction::Install { dir, no_enable, .. } => {
            if !no_enable
                && (!shared.is_dir()
                    || !web_entry.is_file()
                    || !std::process::Command::new("id")
                        .args(["-u", "sloray"])
                        .output()?
                        .status
                        .success())
            {
                anyhow::bail!(
                    "启用前请完成 provision-debian.sh，准备 sloray 用户、shared 目录与 Web 构建产物；仅渲染可用 --no-enable"
                );
            }
            let written = systemd::write_units(dir, &api_unit, &web_unit, &worker_unit)?;
            for path in &written {
                println!("已写入 {}", path.display());
            }

            if *no_enable {
                println!("\n--no-enable：跳过 systemctl。请手动执行：");
                println!("  systemctl daemon-reload");
                println!(
                    "  systemctl enable --now sloray@8080 sloray@8081 sloray-web sloray-worker.timer"
                );
                return Ok(());
            }

            let units = [
                "sloray@8080",
                "sloray@8081",
                "sloray-web",
                "sloray-worker.timer",
            ];
            run_systemctl(&["daemon-reload"])?;
            let mut args = vec!["enable", "--now"];
            args.extend(units);
            run_systemctl(&args)?;

            println!("\n已启用：{}", units.join(", "));
            println!("查看状态：systemctl status sloray@8080");
            println!("未完成初始配置的话，从日志里取 setup token：");
            println!("  journalctl -u sloray@8080 | grep 'SETUP TOKEN'");
            Ok(())
        }
    }
}

fn run_systemctl(args: &[&str]) -> anyhow::Result<()> {
    let output = std::process::Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("执行 systemctl 失败（是否在 systemd 环境里？）：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("systemctl {} 失败：{}", args.join(" "), stderr.trim());
    }
    Ok(())
}

// ---------------------------------------------------------------- setup token

fn setup_token_command(path: &std::path::Path, rotate: bool) -> anyhow::Result<()> {
    let token = if rotate {
        let token = auth::generate_token();
        setup::write_token(path, &token)?;
        eprintln!("已轮换 setup token（旧的立即失效）");
        token
    } else {
        setup::load_or_create_token(path)?
    };

    // token 打到 stdout，提示打到 stderr：
    // 这样可以 `TOKEN=$(sloray setup-token)` 直接取值而不混入提示。
    println!("{token}");
    eprintln!("文件：{}", path.display());
    Ok(())
}
