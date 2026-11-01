use crate::shared::api::Api;
use lemmy_api_common::{
    community::{FollowCommunity, GetCommunity, GetCommunityResponse},
    lemmy_db_schema::newtypes::CommunityId,
    lemmy_db_views_actor::structs::CommunityView,
};

pub async fn get(api: &Api, id: CommunityId) -> Result<CommunityView, String> {
    let result: GetCommunityResponse = api
        .get(
            "community",
            &GetCommunity {
                id: Some(id),
                name: None,
            },
        )
        .await?;
    if result.community_view.community.id != id {
        return Err("The instance returned a different community.".into());
    }
    Ok(result.community_view)
}

pub async fn follow(api: Api, id: CommunityId, follow: bool) -> Result<CommunityView, String> {
    let result: GetCommunityResponse = api
        .post(
            "community/follow",
            &FollowCommunity {
                community_id: id,
                follow,
            },
        )
        .await?;
    if result.community_view.community.id != id {
        return Err("The instance returned a different community.".into());
    }
    Ok(result.community_view)
}
