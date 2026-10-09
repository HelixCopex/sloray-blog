//! 独立页面业务；只有管理员可维护，草稿与删除页面不对外提供。
use crate::{Actor, Error, Result, page_dto::*, query};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SUMMARY: &str = "id,slug,title,summary,status,show_in_nav,nav_order,version,updated_at";
const DETAIL: &str =
    "id,slug,title,summary,body_md,status,show_in_nav,nav_order,version,updated_at";

fn administrator(actor: Actor) -> Result<()> {
    if actor.admin {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}

fn validate(input: &WritePage) -> Result<()> {
    // 独立页面位于根路径；预留应用路由和静态资源前缀，避免发布后无法访问。
    let reserved = [
        "admin", "api", "posts", "tags", "search", "login", "logout", "setup", "healthz", "readyz",
        "_astro", "404", "rss", "sitemap",
    ];
    if !crate::posts::valid_slug(&input.slug)
        || reserved
            .iter()
            .any(|slug| input.slug.eq_ignore_ascii_case(slug))
        || input.title.trim().is_empty()
        || input.title.chars().count() > 200
        || input.summary.chars().count() > 1000
        || input.body_md.len() > 512_000
        || !["draft", "published"].contains(&input.status.as_str())
    {
        return Err(Error::Validation(
            "页面地址、标题、正文或状态不合法（地址不能使用系统保留路径）".into(),
        ));
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Cursor {
    order: i32,
    id: Uuid,
    public: bool,
}

pub async fn list(
    db: &DatabaseConnection,
    public: bool,
    cursor: Option<&str>,
    limit: u32,
) -> Result<PageList> {
    let cursor = cursor
        .map(|value| {
            if value.len() > 1000 {
                return Err(Error::Validation("游标过长".into()));
            }
            let bytes = hex::decode(value).map_err(|_| Error::Validation("游标无效".into()))?;
            let cursor: Cursor =
                serde_json::from_slice(&bytes).map_err(|_| Error::Validation("游标无效".into()))?;
            if cursor.public != public {
                return Err(Error::Validation("游标与页面范围不匹配".into()));
            }
            Ok(cursor)
        })
        .transpose()?;
    let limit = limit.clamp(1, 50) as usize;
    let mut items: Vec<PageSummary> = query::all(db, &format!("SELECT {SUMMARY} FROM pages WHERE deleted_at IS NULL AND (NOT $1 OR status='published') AND ($2::int IS NULL OR (nav_order,id)>($2,$3::uuid)) ORDER BY nav_order,id LIMIT $4"), vec![public.into(),cursor.as_ref().map(|c|c.order).into(),cursor.as_ref().map(|c|c.id).into(),((limit+1) as i64).into()]).await?;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        let last = items.last().ok_or(Error::NotFound)?;
        Some(hex::encode(
            serde_json::to_vec(&Cursor {
                order: last.nav_order,
                id: last.id,
                public,
            })
            .map_err(|e| Error::Validation(e.to_string()))?,
        ))
    } else {
        None
    };
    Ok(PageList { items, next_cursor })
}

pub async fn admin_list(
    db: &DatabaseConnection,
    actor: Actor,
    cursor: Option<&str>,
    limit: u32,
) -> Result<PageList> {
    administrator(actor)?;
    list(db, false, cursor, limit).await
}

pub async fn by_slug(db: &DatabaseConnection, slug: &str) -> Result<Page> {
    query::one(
        db,
        &format!(
            "SELECT {DETAIL} FROM pages WHERE slug=$1 AND status='published' AND deleted_at IS NULL"
        ),
        vec![slug.to_owned().into()],
    )
    .await?
    .ok_or(Error::NotFound)
}

pub async fn get(db: &DatabaseConnection, actor: Actor, id: Uuid) -> Result<Page> {
    administrator(actor)?;
    query::one(
        db,
        &format!("SELECT {DETAIL} FROM pages WHERE id=$1 AND deleted_at IS NULL"),
        vec![id.into()],
    )
    .await?
    .ok_or(Error::NotFound)
}

pub async fn save(
    db: &DatabaseConnection,
    actor: Actor,
    id: Option<Uuid>,
    input: WritePage,
) -> Result<Page> {
    administrator(actor)?;
    validate(&input)?;
    let mut values = vec![
        input.slug.into(),
        input.title.into(),
        input.summary.into(),
        input.body_md.into(),
        input.status.into(),
        input.show_in_nav.into(),
        input.nav_order.into(),
    ];
    let sql = if let Some(id) = id {
        get(db, actor, id).await?;
        let version = input
            .version
            .ok_or_else(|| Error::Validation("更新必须提供 version".into()))?;
        values.extend([id.into(), version.into()]);
        format!(
            "UPDATE pages SET slug=$1,title=$2,summary=$3,body_md=$4,status=$5,show_in_nav=$6,nav_order=$7,version=version+1,updated_at=now() WHERE id=$8 AND version=$9 AND deleted_at IS NULL RETURNING {DETAIL}"
        )
    } else {
        values.push(Uuid::now_v7().into());
        format!(
            "INSERT INTO pages(slug,title,summary,body_md,status,show_in_nav,nav_order,id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING {DETAIL}"
        )
    };
    query::one(db, &sql, values).await?.ok_or(Error::Conflict)
}

pub async fn delete(db: &DatabaseConnection, actor: Actor, id: Uuid, version: i32) -> Result<()> {
    get(db, actor, id).await?;
    if query::exec(db, "UPDATE pages SET deleted_at=now(),updated_at=now(),version=version+1 WHERE id=$1 AND version=$2 AND deleted_at IS NULL", vec![id.into(),version.into()]).await? != 1 { return Err(Error::Conflict); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_titles_and_markdown_byte_limits_are_enforced() {
        let input = WritePage {
            slug: "合法页面".into(),
            title: "文".repeat(200),
            summary: "摘".repeat(1000),
            body_md: "x".repeat(512_000),
            status: "published".into(),
            show_in_nav: true,
            nav_order: i32::MIN,
            version: None,
        };
        assert!(validate(&input).is_ok());
        let mut invalid = input;
        invalid.title.push('文');
        assert!(validate(&invalid).is_err());
        invalid.title = " ".into();
        assert!(validate(&invalid).is_err());
        invalid.title = "合法".into();
        invalid.body_md.push('文');
        assert!(validate(&invalid).is_err());
        invalid.body_md.clear();
        invalid.status = "scheduled".into();
        assert!(validate(&invalid).is_err());
    }

    #[test]
    fn page_paths_cannot_shadow_system_routes() {
        let mut input = WritePage {
            slug: "about".into(),
            title: "关于".into(),
            summary: String::new(),
            body_md: String::new(),
            status: "draft".into(),
            show_in_nav: true,
            nav_order: 0,
            version: None,
        };
        assert!(validate(&input).is_ok());
        for slug in ["admin", "LOGIN", "api", "_astro", "a/b", "rss.xml", ""] {
            input.slug = slug.into();
            assert!(validate(&input).is_err(), "{slug}");
        }
    }
}
