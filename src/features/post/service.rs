use super::{Action, CommentTarget};
use crate::shared::api::Api;
use lemmy_api_common::{
    comment::{
        CommentResponse, CreateComment, CreateCommentLike, GetComment, GetComments,
        GetCommentsResponse,
    },
    lemmy_db_schema::{
        CommentSortType, ListingType,
        newtypes::{CommentId, PostId},
    },
    lemmy_db_views::structs::{CommentView, PostView},
    post::{CreatePostLike, PostResponse, SavePost},
};

pub const PAGE_SIZE: usize = 50;

pub async fn create_comment(
    api: Api,
    post_id: PostId,
    target: CommentTarget,
    content: String,
) -> Result<CommentView, String> {
    let parent_id = match target {
        CommentTarget::Post => None,
        CommentTarget::Reply(id) => Some(id),
    };
    let response: CommentResponse = api
        .post(
            "comment",
            &CreateComment {
                content,
                post_id,
                parent_id,
                language_id: None,
            },
        )
        .await?;
    if response.comment_view.comment.post_id != post_id {
        return Err("The instance returned a comment for a different post.".into());
    }
    if let Some(parent_id) = parent_id
        && response
            .comment_view
            .comment
            .path
            .rsplit('.')
            .nth(1)
            .and_then(|id| id.parse().ok())
            != Some(parent_id.0)
    {
        return Err("The instance returned a reply to a different comment.".into());
    }
    Ok(response.comment_view)
}

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

pub async fn comments(
    api: Api,
    post: PostId,
    page: i64,
    sort: CommentSortType,
) -> Result<GetCommentsResponse, String> {
    api.get(
        "comment/list",
        &GetComments {
            post_id: Some(post),
            // A post's thread should be visible regardless of the default feed scope.
            type_: Some(ListingType::All),
            sort: Some(sort),
            page: Some(page),
            limit: Some(PAGE_SIZE as i64),
            ..Default::default()
        },
    )
    .await
}

pub async fn get_comment(api: Api, id: CommentId, post: PostId) -> Result<CommentView, String> {
    let response: CommentResponse = api.get("comment", &GetComment { id }).await?;
    if response.comment_view.comment.id != id || response.comment_view.comment.post_id != post {
        return Err("The instance returned a different comment.".into());
    }
    Ok(response.comment_view)
}

pub async fn vote(api: Api, comment_id: CommentId, score: i16) -> Result<CommentView, String> {
    let response: CommentResponse = api
        .post("comment/like", &CreateCommentLike { comment_id, score })
        .await?;
    if response.comment_view.comment.id != comment_id {
        return Err("The instance returned a different comment.".into());
    }
    Ok(response.comment_view)
}
