use super::Sort;
use crate::shared::api::Api;
use lemmy_api_common::{
    lemmy_db_schema::ListingType,
    lemmy_db_views::structs::PaginationCursor,
    post::{GetPosts, GetPostsResponse},
};

pub async fn load(
    api: Api,
    sort: Sort,
    local: bool,
    cursor: Option<PaginationCursor>,
) -> Result<GetPostsResponse, String> {
    api.get(
        "post/list",
        &GetPosts {
            type_: Some(if local {
                ListingType::Local
            } else {
                ListingType::All
            }),
            sort: Some(sort.api()),
            limit: Some(20),
            page_cursor: cursor,
            show_nsfw: Some(false),
            ..Default::default()
        },
    )
    .await
}
