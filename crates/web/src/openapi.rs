use crate::routes::{activity, content, media, search, session, site, site_pages, users};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    components(schemas(crate::error::ErrorBody, crate::error::ErrorDetail)),
    paths(
        content::list_posts,
        content::get_post,
        content::list_tags,
        content::admin_posts,
        content::admin_post,
        content::create_post,
        content::update_post,
        content::delete_post,
        content::list_revisions,
        site_pages::list,
        site_pages::public_page,
        site_pages::admin_list,
        site_pages::detail,
        site_pages::create,
        site_pages::update,
        site_pages::delete,
        session::session,
        activity::list,
        activity::create,
        activity::queue,
        activity::moderate,
        activity::get_stats,
        activity::view,
        activity::like,
        activity::unlike,
        search::search,
        media::list,
        media::presign,
        media::register,
        media::get_media,
        media::public_media,
        media::delete,
        site::site,
        users::list,
        users::create
    )
)]
pub struct ApiDoc;

pub fn document() -> utoipa::openapi::OpenApi {
    let mut document = serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI serializes");
    document["components"]["securitySchemes"] = serde_json::json!({
        "session": {"type":"apiKey","in":"cookie","name":"sloray_session"}
    });
    for (path, item) in document["paths"].as_object_mut().expect("OpenAPI paths") {
        for (method, operation) in item.as_object_mut().expect("path item") {
            if !["get", "post", "put", "delete", "patch"].contains(&method.as_str()) {
                continue;
            }
            if path.starts_with("/api/v1/admin/") || path.ends_with("/like") {
                operation["security"] = serde_json::json!([{"session":[]}]);
            }
            if ["post", "put", "delete", "patch"].contains(&method.as_str()) {
                let parameters = operation
                    .as_object_mut()
                    .expect("operation")
                    .entry("parameters")
                    .or_insert_with(|| serde_json::json!([]))
                    .as_array_mut()
                    .expect("parameters");
                parameters.push(serde_json::json!({"in":"header","name":"X-CSRF-Token","required":true,"schema":{"type":"string"},"description":"从 /api/v1/auth/session 获取，必须同时携带返回的 CSRF Cookie"}));
            }
            for status in [
                "400", "401", "403", "404", "409", "413", "415", "422", "429", "500", "503",
            ] {
                operation["responses"][status] = serde_json::json!({"description":"统一错误信封","content":{"application/json":{"schema":{"$ref":"#/components/schemas/ErrorBody"}}}});
            }
        }
    }
    serde_json::from_value(document).expect("valid OpenAPI document")
}
