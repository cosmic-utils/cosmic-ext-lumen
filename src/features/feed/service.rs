use super::{Scope, Sort};
use crate::shared::api::Api;
use lemmy_api_common::{
    comment::{GetComments, GetCommentsResponse},
    lemmy_db_schema::{CommentSortType, ListingType, newtypes::PersonId},
    lemmy_db_views::structs::{CommentView, PaginationCursor},
    person::{GetPersonDetails, GetPersonDetailsResponse},
    post::{GetPosts, GetPostsResponse},
    site::{Search, SearchResponse},
};

pub const PAGE_SIZE: i64 = 20;

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
                Scope::All
                | Scope::Saved
                | Scope::Community(_)
                | Scope::Search
                | Scope::Profile(_) => ListingType::All,
                Scope::Local => ListingType::Local,
                Scope::Subscribed => ListingType::Subscribed,
            }),
            saved_only: (scope == Scope::Saved).then_some(true),
            sort: Some(sort.api()),
            limit: Some(PAGE_SIZE),
            community_id: match scope {
                Scope::Community(id) => Some(id),
                _ => None,
            },
            page_cursor: cursor,
            show_nsfw: Some(false),
            ..Default::default()
        },
    )
    .await
}

pub async fn search(api: Api, query: Search) -> Result<SearchResponse, String> {
    api.get("search", &query).await
}

pub async fn saved_comments(api: Api, page: i64) -> Result<Vec<CommentView>, String> {
    let response: GetCommentsResponse = api
        .get(
            "comment/list",
            &GetComments {
                type_: Some(ListingType::All),
                sort: Some(CommentSortType::New),
                saved_only: Some(true),
                page: Some(page),
                limit: Some(PAGE_SIZE),
                ..Default::default()
            },
        )
        .await?;
    Ok(response.comments)
}

pub async fn profile(
    api: Api,
    id: PersonId,
    sort: Sort,
    page: i64,
) -> Result<GetPersonDetailsResponse, String> {
    let response: GetPersonDetailsResponse = api
        .get(
            "user",
            &GetPersonDetails {
                person_id: Some(id),
                sort: Some(sort.api()),
                page: Some(page),
                limit: Some(PAGE_SIZE),
                ..Default::default()
            },
        )
        .await?;
    if response.person_view.person.id != id {
        return Err("The instance returned a different person.".into());
    }
    Ok(response)
}
