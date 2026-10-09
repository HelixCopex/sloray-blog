//! 对外用户 DTO 与创建校验；密码哈希永不进入契约。
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
#[derive(Serialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}
impl From<sloray_store::entity::users::Model> for User {
    fn from(user: sloray_store::entity::users::Model) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            role: user.role,
            created_at: user.created_at.into(),
        }
    }
}
#[derive(Deserialize, ToSchema)]
pub struct CreateUser {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub role: String,
}
pub fn validate(input: &CreateUser) -> Result<(), String> {
    crate::validate_password(&input.password)?;
    if input.email.len() > 254
        || input
            .email
            .split_once('@')
            .is_none_or(|(user, host)| user.is_empty() || host.is_empty())
        || input.email.chars().any(char::is_whitespace)
    {
        return Err("邮箱格式不正确".into());
    }
    if input.display_name.trim().is_empty()
        || input.display_name.chars().count() > 80
        || !["author", "admin"].contains(&input.role.as_str())
    {
        return Err("昵称或角色不合法".into());
    }
    Ok(())
}
pub async fn list(db: &DatabaseConnection) -> Result<Vec<User>, sea_orm::DbErr> {
    use sloray_store::entity::users;
    Ok(users::Entity::find()
        .order_by_asc(users::Column::CreatedAt)
        .limit(1000)
        .all(db)
        .await?
        .into_iter()
        .map(User::from)
        .collect())
}
