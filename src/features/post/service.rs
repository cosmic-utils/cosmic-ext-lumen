use super::Action;
use crate::shared::api::Api;
use lemmy_api_common::{
    comment::{GetComments, GetCommentsResponse},
    lemmy_db_schema::{CommentSortType, newtypes::PostId},
    lemmy_db_views::structs::PostView,
    post::{CreatePostLike, PostResponse, SavePost},
};

pub const PAGE_SIZE: usize = 50;

pub async fn act(api: Api, post: &PostView, action: Action) -> Result<PostView, String> {
    let post_id = post.post.id;
    let response: PostResponse = match action {
        Action::Upvote | Action::Downvote => {
            let vote = if matches!(action, Action::Upvote) {
                1
            } else {
                -1
            };
            let score = if post.my_vote == Some(vote) { 0 } else { vote };
            api.post("post/like", &CreatePostLike { post_id, score })
                .await?
        }
        Action::ToggleSaved => {
            api.put(
                "post/save",
                &SavePost {
                    post_id,
                    save: !post.saved,
                },
            )
            .await?
        }
    };
    if response.post_view.post.id != post_id {
        return Err("The instance returned a different post.".into());
    }
    Ok(response.post_view)
}

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
