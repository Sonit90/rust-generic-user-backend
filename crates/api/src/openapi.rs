use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::routes::{admin, auth, files, formats, mappings, merge, users};

#[derive(OpenApi)]
#[openapi(
    paths(
        auth::register,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::verify_email,
        auth::resend_verification,
        auth::forgot_password,
        auth::reset_password,
        auth::google_start,
        auth::google_callback,
        users::me,
        users::my_permissions,
        admin::list_users,
        admin::set_role,
        admin::set_active,
        admin::grant_permission,
        files::upload,
        files::list,
        files::download,
        files::delete_file,
        files::preview,
        mappings::create,
        mappings::list,
        mappings::get_one,
        mappings::delete,
        formats::create,
        formats::list,
        formats::get_one,
        formats::delete,
        formats::update,
        merge::start_run,
        merge::list_runs,
        merge::get_run,
        merge::download_run,
    ),
    components(schemas(
        auth::RegisterReq,
        auth::TokenPair,
        auth::LoginReq,
        auth::RefreshReq,
        auth::LogoutReq,
        auth::OAuthCallbackQuery,
        auth::ForgotPasswordReq,
        auth::ResetPasswordReq,
        admin::Page,
        admin::SetRoleReq,
        admin::SetActiveReq,
        admin::GrantPermReq,
        mappings::CreateMappingReq,
        formats::CreateFormatReq,
        formats::UpdateFormatReq,
        merge::StartRunReq,
        merge::InputSpec,
        price_merger_core::models::User,
        price_merger_core::models::Role,
        price_merger_core::models::UploadedFile,
        price_merger_core::models::FileKind,
        price_merger_core::models::ColumnMapping,
        price_merger_core::models::MappedColumn,
        price_merger_core::models::DataType,
        price_merger_core::models::CanonicalColumn,
        price_merger_core::models::ColumnTransform,
        price_merger_core::models::OutputFormat,
        price_merger_core::models::OutputColumn,
        price_merger_core::models::ExprTransform,
        price_merger_core::models::MergeRun,
        price_merger_core::models::MergeStatus,
        files::FilePreviewResponse,
    )),
    modifiers(&BearerAuth),
    info(
        title = "Price Merger API",
        version = "0.1.0",
    ),
)]
pub struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}
