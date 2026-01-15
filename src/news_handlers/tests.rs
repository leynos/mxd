//! Unit tests for news handler helpers.

use rstest::fixture;

use super::*;

/// Returns a `PostArticleRequest` with sensible default values for testing.
#[fixture]
fn default_post_article_request() -> PostArticleRequest {
    PostArticleRequest {
        path: "/news".to_string(),
        title: "Test Article".to_string(),
        flags: 0,
        data_flavor: "text/plain".to_string(),
        data: "Test content".to_string(),
    }
}

#[test]
fn post_article_request_to_db_params_maps_title() {
    let req = PostArticleRequest {
        title: "Hello World".to_string(),
        ..default_post_article_request()
    };
    let params = req.to_db_params();
    assert_eq!(params.title, "Hello World");
}

#[test]
fn post_article_request_to_db_params_maps_flags() {
    let req = PostArticleRequest {
        flags: 42,
        ..default_post_article_request()
    };
    let params = req.to_db_params();
    assert_eq!(params.flags, 42);
}

#[test]
fn post_article_request_to_db_params_maps_data_flavor() {
    let req = PostArticleRequest {
        data_flavor: "text/html".to_string(),
        ..default_post_article_request()
    };
    let params = req.to_db_params();
    assert_eq!(params.data_flavor, "text/html");
}

#[test]
fn post_article_request_to_db_params_maps_data() {
    let req = PostArticleRequest {
        data: "Article body content".to_string(),
        ..default_post_article_request()
    };
    let params = req.to_db_params();
    assert_eq!(params.data, "Article body content");
}

#[test]
fn post_article_request_to_db_params_excludes_path() {
    let req = PostArticleRequest {
        path: "/news/category".to_string(),
        ..default_post_article_request()
    };
    let params = req.to_db_params();
    // Path is not part of CreateRootArticleParams; it's used separately
    // for the path lookup. Verify the other fields are correctly mapped.
    assert_eq!(params.title, "Test Article");
    assert_eq!(params.flags, 0);
    assert_eq!(params.data_flavor, "text/plain");
    assert_eq!(params.data, "Test content");
}
