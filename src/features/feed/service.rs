use super::{Scope, Sort};
use crate::shared::api::Api;
use lemmy_api_common::{
    lemmy_db_schema::ListingType,
    lemmy_db_views::structs::PaginationCursor,
    post::{GetPosts, GetPostsResponse},
};

pub async fn load(
    api: Api,
    sort: Sort,
    scope: Scope,
    cursor: Option<PaginationCursor>,
) -> Result<GetPostsResponse, String> {
    api.get(
        "post/list",
        &GetPosts {
            type_: Some(match scope {
                Scope::All | Scope::Saved => ListingType::All,
                Scope::Local => ListingType::Local,
                Scope::Subscribed => ListingType::Subscribed,
            }),
            saved_only: (scope == Scope::Saved).then_some(true),
            sort: Some(sort.api()),
            limit: Some(20),
            page_cursor: cursor,
            show_nsfw: Some(false),
            ..Default::default()
        },
    )
    .await
}
