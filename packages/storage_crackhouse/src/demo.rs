
use crate::models::{NewPost, Post};
use diesel::prelude::*;

use crate::establish_connection;
use crate::schema;



pub fn create_post(title: &str, body: &str) -> anyhow::Result<Post> {
    use crate::schema::posts;

    let new_post = NewPost { title, body };

    let c = &mut establish_connection();

    let post = diesel::insert_into(posts::table)
        .values(&new_post)
        .returning(Post::as_returning())
        .get_result(c)
        ?;

    Ok(post)
}

pub fn delete_post(pattern: &str) -> anyhow::Result<()>{
    let connection = &mut establish_connection();
    let num_deleted = diesel::delete(
        crate::schema::posts::dsl::posts.filter(crate::schema::posts::title.like(pattern.to_string())),
    )
    .execute(connection)
    ?;

    tracing::info!("Deleted {num_deleted} posts");
    Ok(())
}

// pub fn get_post(post_id: i32) -> Option<Post> {
//     use schema::posts::dsl::posts;

//     let connection = &mut establish_connection();

//     let post = posts
//         .find(post_id)
//         .select(Post::as_select())
//         .first(connection)
//         .optional(); // This allows for returning an Option<Post>, otherwise it will throw an error

//     match &post {
//         Ok(Some(post)) => tracing::info!("Post with id: {} has a title: {}", post.id, post.title),
//         Ok(None) => tracing::info!("Unable to find post {}", post_id),
//         Err(_) => tracing::info!("An error occurred while fetching post {}", post_id),
//     }
//     post.ok().flatten()
// }

// pub fn publish_post(id: i32) {
//     let connection = &mut establish_connection();

//     let post = diesel::update(schema::posts::dsl::posts.find(id))
//         .set(schema::posts::dsl::published.eq(true))
//         .returning(Post::as_returning())
//         .get_result(connection)
//         .unwrap();

//     tracing::info!("Published post {}", post.title);
// }

pub fn show_posts() -> anyhow::Result<Vec<Post>> {
    let connection = &mut establish_connection();
    let results = schema::posts::dsl::posts
        .filter(schema::posts::dsl::published.eq(true))
        .limit(5)
        .select(Post::as_select())
        .load(connection)
        ?;

    tracing::info!("Displaying {} posts", results.len());
    for post in &results {
        tracing::info!("{}", post.title);
        tracing::info!("----------\n");
        tracing::info!("{}", post.body);
    }
    Ok(results)
}