use actix_web::{test, App};
use arcadematrix::api::ota::get_version;
use arcadematrix::api::server::api_fonts;

#[actix_web::test]
async fn test_version_api_route() {
    let app = test::init_service(App::new().service(get_version)).await;
    let req = test::TestRequest::get().uri("/api/version").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_fonts_api_route() {
    let app = test::init_service(App::new().service(api_fonts)).await;
    let req = test::TestRequest::get().uri("/api/fonts").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}
