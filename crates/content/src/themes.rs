//! 主题库与启用快照；Postgres 是唯一权威源，编辑不会隐式发布。
use crate::{Actor, Error, Result, query, theme_dto::*};
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, FromQueryResult, TransactionTrait};
use uuid::Uuid;

pub fn administrator(actor: Actor) -> Result<()> {
    if actor.admin {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}

pub fn validate(package: &ThemePackage) -> Result<()> {
    let layout = &package.layout;
    let blocks = &layout.home_blocks;
    if package.schema_version != 1
        || package.name.trim().is_empty()
        || package.name.chars().count() > 100
        || package.description.chars().count() > 1000
        || package.css.len() > 65_536
        || !["violet", "teal", "blue", "rose"].contains(&package.palette.as_str())
        || !["sans", "serif", "mono"].contains(&package.font.as_str())
        || !(640..=1600).contains(&layout.width)
        || !(1..=3).contains(&layout.columns)
        || !["top", "side"].contains(&layout.navigation.as_str())
        || !["cards", "list"].contains(&layout.posts.as_str())
        || blocks.is_empty()
        || blocks.len() > 4
        || !blocks.iter().any(|b| b == "posts")
        || blocks
            .iter()
            .any(|b| !["hero", "posts", "pages", "widgets"].contains(&b.as_str()))
        || blocks
            .iter()
            .enumerate()
            .any(|(i, b)| blocks[..i].contains(b))
        || layout.hero_title.chars().count() > 200
        || layout.hero_description.chars().count() > 1000
        || layout.footer.chars().count() > 300
    {
        return Err(Error::Validation(
            "主题包版本、颜色、字体、布局或长度不合法".into(),
        ));
    }
    validate_rows(&layout.home_rows)?;
    Ok(())
}

fn validate_rows(rows: &[HomeRow]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    if rows.len() > 12
        || rows.iter().filter(|r| r.kind == "posts").count() != 1
        || rows.iter().filter(|r| r.kind == "hero").count() > 1
        || rows.iter().any(|r| {
            !["hero", "posts", "widgets"].contains(&r.kind.as_str())
                || r.title.chars().count() > 100
                || r.widgets.len() > 4
                || (r.kind != "widgets" && !r.widgets.is_empty())
                || (r.kind == "widgets" && r.widgets.is_empty())
                || r.widgets.iter().any(|w| {
                    !["note", "pages", "tags", "subscribe"].contains(&w.kind.as_str())
                        || w.title.chars().count() > 100
                        || w.text.chars().count() > 1000
                })
        })
    {
        return Err(Error::Validation(
            "首页须含一个文章区块，最多 12 行；组件域须含 1–4 个受支持的小组件".into(),
        ));
    }
    Ok(())
}

#[derive(FromQueryResult)]
struct Record {
    id: Uuid,
    package: serde_json::Value,
    version: i32,
    updated_at: DateTime<Utc>,
}
impl Record {
    fn dto(self) -> Result<Theme> {
        Ok(Theme {
            id: self.id,
            package: serde_json::from_value(self.package)
                .map_err(|e| Error::Validation(e.to_string()))?,
            version: self.version,
            updated_at: self.updated_at,
        })
    }
}
#[derive(FromQueryResult)]
struct StateRecord {
    active_theme_id: Option<Uuid>,
    package: Option<serde_json::Value>,
    version: i32,
}
impl StateRecord {
    fn dto(self) -> Result<ThemeState> {
        Ok(ThemeState {
            active_theme_id: self.active_theme_id,
            package: self
                .package
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| Error::Validation(e.to_string()))?
                .unwrap_or_default(),
            version: self.version,
        })
    }
}

pub async fn state(db: &DatabaseConnection) -> Result<ThemeState> {
    query::one::<StateRecord, _>(
        db,
        "SELECT active_theme_id,package,version FROM theme_settings WHERE id=1",
        vec![],
    )
    .await?
    .ok_or(Error::NotFound)?
    .dto()
}

pub async fn list(db: &DatabaseConnection, actor: Actor) -> Result<Vec<Theme>> {
    administrator(actor)?;
    query::all::<Record, _>(db, "SELECT id,package,version,updated_at FROM themes WHERE deleted_at IS NULL ORDER BY updated_at DESC,id", vec![])
        .await?.into_iter().map(Record::dto).collect()
}

pub async fn save(
    db: &DatabaseConnection,
    actor: Actor,
    id: Option<Uuid>,
    input: WriteTheme,
) -> Result<Theme> {
    administrator(actor)?;
    validate(&input.package)?;
    let package =
        serde_json::to_value(input.package).map_err(|e| Error::Validation(e.to_string()))?;
    let record: Option<Record> = if let Some(id) = id {
        let version = input
            .version
            .ok_or_else(|| Error::Validation("更新必须提供 version".into()))?;
        let exists: Option<Record> = query::one(
            db,
            "SELECT id,package,version,updated_at FROM themes WHERE id=$1 AND deleted_at IS NULL",
            vec![id.into()],
        )
        .await?;
        exists.ok_or(Error::NotFound)?;
        query::one(db, "UPDATE themes SET package=$1,version=version+1,updated_at=now() WHERE id=$2 AND version=$3 AND deleted_at IS NULL RETURNING id,package,version,updated_at", vec![package.into(),id.into(),version.into()]).await?
    } else {
        query::one(
            db,
            "INSERT INTO themes(id,package) VALUES($1,$2) RETURNING id,package,version,updated_at",
            vec![Uuid::now_v7().into(), package.into()],
        )
        .await?
    };
    record.ok_or(Error::Conflict)?.dto()
}

pub async fn activate(
    db: &DatabaseConnection,
    actor: Actor,
    input: ActivateTheme,
) -> Result<ThemeState> {
    administrator(actor)?;
    let txn = db.begin().await?;
    // 启用和删除采用同一锁顺序，防止启用已删除的主题或丢失并发修改。
    query::one::<StateRecord, _>(
        &txn,
        "SELECT active_theme_id,package,version FROM theme_settings WHERE id=1 FOR UPDATE",
        vec![],
    )
    .await?
    .ok_or(Error::NotFound)?;
    let package = if let Some(id) = input.theme_id {
        let theme: Record = query::one(&txn, "SELECT id,package,version,updated_at FROM themes WHERE id=$1 AND deleted_at IS NULL FOR SHARE", vec![id.into()]).await?.ok_or(Error::NotFound)?;
        let version = input
            .theme_version
            .ok_or_else(|| Error::Validation("启用必须提供 theme_version".into()))?;
        if theme.version != version {
            return Err(Error::Conflict);
        }
        Some(theme.package)
    } else {
        None
    };
    let result: StateRecord = query::one(&txn, "UPDATE theme_settings SET active_theme_id=$1,package=$2,version=version+1 WHERE id=1 AND version=$3 RETURNING active_theme_id,package,version", vec![input.theme_id.into(),package.into(),input.version.into()]).await?.ok_or(Error::Conflict)?;
    txn.commit().await?;
    result.dto()
}

pub async fn delete(db: &DatabaseConnection, actor: Actor, id: Uuid, version: i32) -> Result<()> {
    administrator(actor)?;
    let txn = db.begin().await?;
    let state: StateRecord = query::one(
        &txn,
        "SELECT active_theme_id,package,version FROM theme_settings WHERE id=1 FOR UPDATE",
        vec![],
    )
    .await?
    .ok_or(Error::NotFound)?;
    if state.active_theme_id == Some(id) {
        return Err(Error::Validation("请先切换或恢复默认主题，再删除".into()));
    }
    let exists: Record = query::one(
        &txn,
        "SELECT id,package,version,updated_at FROM themes WHERE id=$1 AND deleted_at IS NULL",
        vec![id.into()],
    )
    .await?
    .ok_or(Error::NotFound)?;
    if exists.version != version {
        return Err(Error::Conflict);
    }
    if query::exec(&txn, "UPDATE themes SET deleted_at=now(),version=version+1,updated_at=now() WHERE id=$1 AND version=$2 AND deleted_at IS NULL", vec![id.into(),version.into()]).await? != 1 { return Err(Error::Conflict); }
    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_and_all_presets_are_valid() {
        let mut package = ThemePackage::default();
        for palette in ["violet", "teal", "blue", "rose"] {
            package.palette = palette.into();
            assert!(validate(&package).is_ok());
        }
    }
    #[test]
    fn invalid_layouts_and_versions_are_rejected() {
        let base = ThemePackage::default();
        for blocks in [
            vec![],
            vec!["hero"],
            vec!["posts", "posts"],
            vec!["posts", "script"],
        ] {
            let mut p = base.clone();
            p.layout.home_blocks = blocks.into_iter().map(String::from).collect();
            assert!(validate(&p).is_err());
        }
        for width in [0, 639, 1601, u32::MAX] {
            let mut p = base.clone();
            p.layout.width = width;
            assert!(validate(&p).is_err());
        }
        let mut p = base.clone();
        p.schema_version = 2;
        assert!(validate(&p).is_err());
        p = base.clone();
        p.css = "a".repeat(65_537);
        assert!(validate(&p).is_err());
        p = base;
        p.name = " ".into();
        assert!(validate(&p).is_err());
    }
    #[test]
    fn packages_cannot_contain_executable_fields() {
        let mut json = serde_json::to_value(ThemePackage::default()).unwrap();
        json["javascript"] = "alert(1)".into();
        assert!(serde_json::from_value::<ThemePackage>(json).is_err());
    }
    #[test]
    fn home_rows_require_one_post_list_and_bounded_passive_widgets() {
        let mut package = ThemePackage::default();
        package.layout.home_rows = vec![
            HomeRow {
                kind: "posts".into(),
                title: String::new(),
                widgets: vec![],
            },
            HomeRow {
                kind: "widgets".into(),
                title: "探索".into(),
                widgets: vec![HomeWidget {
                    kind: "note".into(),
                    title: "便笺".into(),
                    text: "纯文字".into(),
                }],
            },
        ];
        assert!(validate(&package).is_ok());
        package.layout.home_rows[1].widgets[0].kind = "script".into();
        assert!(validate(&package).is_err());
        package.layout.home_rows[1].widgets[0].kind = "tags".into();
        package.layout.home_rows[0].kind = "hero".into();
        assert!(validate(&package).is_err());
    }
    #[test]
    fn author_cannot_manage_themes() {
        assert!(matches!(
            administrator(Actor {
                id: Uuid::nil(),
                admin: false
            }),
            Err(Error::Forbidden)
        ));
    }
}
