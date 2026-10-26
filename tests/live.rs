//! Optional public, read-only smoke test. Never runs in the default suite.
use cosmic_ext_lemmy::shared::api::{Api, http_client};
use lemmy_api_common::{
    comment::{GetComments, GetCommentsResponse},
    post::{GetPosts, GetPostsResponse},
    site::GetSiteResponse,
};

#[tokio::test]
#[ignore = "requires network access to a Lemmy 0.19.x instance"]
async fn reads_instance_feed_and_comments() {
    let instance =
        std::env::var("LEMMY_TEST_INSTANCE").unwrap_or_else(|_| "https://lemmy.world".into());
    let api = Api::new(http_client().unwrap(), &instance).unwrap();
    let site: GetSiteResponse = api.get("site", &()).await.unwrap();
    assert!(
        site.version.starts_with("0.19."),
        "Expected API v3 server, found {}",
        site.version
    );
    let posts: GetPostsResponse = api
        .get(
            "post/list",
            &GetPosts {
                limit: Some(2),
                show_nsfw: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        !posts.posts.is_empty(),
        "Choose an instance with public posts"
    );
    let comments: GetCommentsResponse = api
        .get(
            "comment/list",
            &GetComments {
                post_id: Some(posts.posts[0].post.id),
                limit: Some(5),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    println!(
        "Lemmy {}: read {} posts and {} comments",
        site.version,
        posts.posts.len(),
        comments.comments.len()
    );
}
