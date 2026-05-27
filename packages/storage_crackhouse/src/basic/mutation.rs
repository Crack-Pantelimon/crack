use super::*;
use sea_orm::{DbConn, entity::*, error::*};

pub fn all_about_mutation(db: &DbConn) -> Result<(), DbErr> {
    insert_and_update(db)?;

    tracing::info!("===== =====\n");

    save_active_model(db)?;

    tracing::info!("===== =====\n");

    save_custom_active_model(db)?;

    Ok(())
}

pub fn insert_and_update(db: &DbConn) -> Result<(), DbErr> {
    let pear = fruit::ActiveModel {
        name: Set("pear".to_owned()),
        ..Default::default()
    };
    let res = Fruit::insert(pear).exec(db)?;

    tracing::info!("Inserted: last_insert_id = {}", res.last_insert_id);

    let pear: Option<fruit::Model> = Fruit::find_by_id(res.last_insert_id).one(db)?;

    tracing::info!("Pear: {pear:?}");

    let mut pear: fruit::ActiveModel = pear.unwrap().into();
    pear.name = Set("Sweet pear".to_owned());

    let pear: fruit::Model = pear.update(db)?;

    tracing::info!("Updated: {pear:?}");

    let result = pear.delete(db)?;

    tracing::info!("Deleted: {result:?}");

    Ok(())
}

pub fn save_active_model(db: &DbConn) -> Result<(), DbErr> {
    let banana = fruit::ActiveModel {
        name: Set("Banana".to_owned()),
        ..Default::default()
    };
    let mut banana: fruit::ActiveModel = banana.save(db)?;

    tracing::info!("Inserted: {banana:?}");

    banana.name = Set("Banana Mongo".to_owned());

    let banana: fruit::ActiveModel = banana.save(db)?;

    tracing::info!("Updated: {banana:?}");

    let result = banana.delete(db)?;

    tracing::info!("Deleted: {result:?}");

    Ok(())
}

mod form {
    use super::fruit::*;
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveIntoActiveModel)]
    pub struct InputModel {
        pub name: String,
    }
}

fn save_custom_active_model(db: &DbConn) -> Result<(), DbErr> {
    let pineapple = form::InputModel {
        name: "Pineapple".to_owned(),
    }
    .into_active_model();

    let pineapple = pineapple.save(db)?;

    tracing::info!("Saved: {pineapple:?}");

    let result = pineapple.delete(db)?;

    tracing::info!("Deleted: {result:?}");

    Ok(())
}
