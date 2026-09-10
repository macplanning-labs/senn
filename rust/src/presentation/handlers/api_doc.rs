/// presentation/handlers/api_doc.rs — OpenAPI ドキュメント定義
///
/// utoipa による自動生成 OpenAPI スキーマを定義し、
/// Swagger UI でドキュメントを表示する。
///
/// まずticket/project/wiki系の一覧・詳細(GET)に絞って導入。
/// POST/PUT(create/update)はリクエストボディ型(TicketWriteIn等)に
/// utoipa::ToSchemaの実装が無く追加できていない。導入する場合は
/// 該当のdomain/models配下の型に#[derive(utoipa::ToSchema)]を追加する。

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::presentation::handlers::tickets_api::list,
        crate::presentation::handlers::tickets_api::detail,
        crate::presentation::handlers::resource_api::project_list,
        crate::presentation::handlers::resource_api::project_detail,
        crate::presentation::handlers::wiki_api::list,
        crate::presentation::handlers::wiki_api::detail,
    ),
    tags(
        (name = "tickets", description = "チケット関連API"),
        (name = "projects", description = "プロジェクト関連API"),
        (name = "wiki", description = "Wiki関連API"),
    )
)]
pub struct ApiDoc;
