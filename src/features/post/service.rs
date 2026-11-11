use super::{Action, CommentAction, CommentTarget};
use crate::fl;
use crate::shared::api::{Api, Freshness};
use lemmy_api_common::{
    comment::{
        CommentResponse, CreateComment, CreateCommentLike, DeleteComment, EditComment, GetComment,
        GetComments, GetCommentsResponse, SaveComment,
    },
    lemmy_db_schema::{
        CommentSortType, ListingType,
        newtypes::{CommentId, PostId},
    },
    lemmy_db_views::structs::{CommentView, PostView},
    post::{CreatePostLike, DeletePost, GetPost, GetPostResponse, PostResponse, SavePost},
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
        CommentTarget::Edit(comment_id) => {
            let response = api
                .put(
                    "comment",
                    &EditComment {
                        comment_id,
                        content: Some(content),
                        language_id: None,
                    },
                )
                .await?;
            return checked_comment(response, post_id, comment_id);
        }
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
        return Err(fl!("comment-post-mismatch"));
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
        return Err(fl!("reply-parent-mismatch"));
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
        Action::SetDeleted(deleted) => {
            api.post("post/delete", &DeletePost { post_id, deleted })
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
        return Err(fl!("post-mismatch"));
    }
    if let Action::SetDeleted(deleted) = action
        && response.post_view.post.deleted != deleted
    {
        return Err(fl!("post-status-unconfirmed"));
    }
    Ok(response.post_view)
}

pub async fn comments(
    api: Api,
    post: PostId,
    page: i64,
    sort: CommentSortType,
    freshness: Freshness,
) -> Result<GetCommentsResponse, String> {
    api.get_cached(
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
        freshness,
    )
    .await
}

pub async fn get_comment(api: Api, id: CommentId, post: PostId) -> Result<CommentView, String> {
    let response: CommentResponse = api
        .get_cached("comment", &GetComment { id }, Freshness::Cached)
        .await?;
    checked_comment(response, post, id)
}

pub async fn act_on_comment(
    api: Api,
    post_id: PostId,
    comment_id: CommentId,
    action: CommentAction,
) -> Result<CommentView, String> {
    let response: CommentResponse = match action {
        CommentAction::Vote(score) => {
            api.post("comment/like", &CreateCommentLike { comment_id, score })
                .await?
        }
        CommentAction::SetDeleted(deleted) => {
            api.post(
                "comment/delete",
                &DeleteComment {
                    comment_id,
                    deleted,
                },
            )
            .await?
        }
        CommentAction::Save(save) => {
            api.put("comment/save", &SaveComment { comment_id, save })
                .await?
        }
    };
    checked_comment(response, post_id, comment_id)
}

fn checked_comment(
    response: CommentResponse,
    post_id: PostId,
    comment_id: CommentId,
) -> Result<CommentView, String> {
    if response.comment_view.comment.id != comment_id
        || response.comment_view.comment.post_id != post_id
    {
        return Err(fl!("comment-mismatch"));
    }
    Ok(response.comment_view)
}

pub async fn get_post(api: &Api, id: PostId) -> Result<PostView, String> {
    let response: GetPostResponse = api
        .get_cached(
            "post",
            &GetPost {
                id: Some(id),
                comment_id: None,
            },
            Freshness::Cached,
        )
        .await?;
    if response.post_view.post.id != id {
        return Err(fl!("post-mismatch"));
    }
    Ok(response.post_view)
}
