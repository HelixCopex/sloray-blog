//! 数据主题包是公开契约，不包含可执行模板或脚本。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemePackage {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub palette: String,
    pub font: String,
    pub layout: ThemeLayout,
    #[serde(default)]
    pub css: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeLayout {
    pub width: u32,
    pub navigation: String,
    pub posts: String,
    pub columns: u32,
    pub home_blocks: Vec<String>,
    pub hero_title: String,
    pub hero_description: String,
    pub footer: String,
    /// 为空时继续解释旧版 home_blocks，保持已发布主题兼容。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub home_rows: Vec<HomeRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HomeRow {
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub widgets: Vec<HomeWidget>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HomeWidget {
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
}

impl Default for ThemePackage {
    fn default() -> Self {
        Self {
            schema_version: 1,
            name: "静线 · Material".into(),
            description: "默认 Material 3 主题".into(),
            palette: "blue".into(),
            font: "sans".into(),
            layout: ThemeLayout {
                width: 1120,
                navigation: "side".into(),
                posts: "list".into(),
                columns: 1,
                home_blocks: vec!["hero".into(), "widgets".into(), "posts".into()],
                hero_title: "记录当下，连接想法。".into(),
                hero_description: "写作、分享，以及值得留下的讨论。".into(),
                footer: "记录、分享与讨论".into(),
                home_rows: vec![],
            },
            css: String::new(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Theme {
    pub id: Uuid,
    pub package: ThemePackage,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ThemeState {
    pub active_theme_id: Option<Uuid>,
    pub package: ThemePackage,
    pub version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteTheme {
    pub package: ThemePackage,
    pub version: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActivateTheme {
    /// null 恢复内置主题；version 为 ThemeState 的版本。
    pub theme_id: Option<Uuid>,
    /// 启用时必须与已预览的主题库版本相同；恢复默认为 null。
    pub theme_version: Option<i32>,
    pub version: i32,
}
