
use _crack_utils::sleep_ms;

use api_asscrack::declare_api_group2;
use api_asscrack::implement_api_group2;


declare_api_group2! {
    StorageCrackhouseApiGroup,
    [
        (CreatePost, (String, String), String),
        (DeletePost, String, ()),
        (ShowPosts, (), Vec<String>),
    ]
}

implement_api_group2! {
    StorageCrackhouseApiGroup,
    [
        (CreatePost, create_post),
        (DeletePost, delete_post),
        (ShowPosts, show_posts),
    ]
}

pub async fn create_post(_x: (String, String)) -> anyhow::Result<String> {
    sleep_ms(1).await;
    let t= crate::demo::create_post(&_x.0, &_x.1)?;
    let t = format!("{t:#?}");
    tracing::info!("Create Post -> {t}");

    Ok(t)
}


pub async fn delete_post(_x: String) -> anyhow::Result<()> {
    sleep_ms(1).await;
    let t= crate::demo::delete_post(&_x)?;
    Ok(t)
}


pub async fn show_posts(_x: ()) -> anyhow::Result<Vec<String>> {
    sleep_ms(1).await;
    let t= crate::demo::show_posts()?;
    tracing::info!("Show Posts: {t:#?}");
    let t = t.into_iter().map(|e| format!("{e:#?}")).collect();

    Ok(t)
}