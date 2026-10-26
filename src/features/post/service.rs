use crate::shared::api::Api;
use lemmy_api_common::{
    comment::{GetComments, GetCommentsResponse},
    lemmy_db_schema::{CommentSortType, newtypes::PostId},
};

pub const PAGE_SIZE: usize = 50;

pub async fn comments(api: Api, post: i32, page: i64) -> Result<GetCommentsResponse, String> {
    api.get(
        "comment/list",
        &GetComments {
            post_id: Some(PostId(post)),
            sort: Some(CommentSortType::Old),
            page: Some(page),
            limit: Some(PAGE_SIZE as i64),
            ..Default::default()
        },
    )
    .await
}
