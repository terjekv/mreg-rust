use actix_web::http::StatusCode;
use rstest::rstest;

use crate::fixtures::{
    ADMIN_GROUP, ADMIN_USER, READONLY_GROUP, READONLY_USER, one_group, treetop_ctx,
};

#[rstest]
#[case::admin_health("/system/health", ADMIN_USER, ADMIN_GROUP, StatusCode::OK)]
#[case::readonly_version("/system/version", READONLY_USER, READONLY_GROUP, StatusCode::OK)]
#[case::readonly_status("/system/status", READONLY_USER, READONLY_GROUP, StatusCode::OK)]
#[case::ungrouped_health("/system/health", "mallory", "", StatusCode::OK)]
#[case::ungrouped_version("/system/version", "mallory", "", StatusCode::OK)]
#[case::ungrouped_status("/system/status", "mallory", "", StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn system_endpoints_match_public_and_protected_access(
    #[case] endpoint: &str,
    #[case] user: &str,
    #[case] group: &str,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    let groups = one_group(group);
    assert_eq!(ctx.get_status_as(endpoint, user, &groups).await, expected);
}
