use super::*;
use anyhow::Context;
use sea_orm::{DbConn, FromQueryResult, entity::*, error::*, query::*};

pub fn all_about_query(db: &DbConn) -> Result<(), DbErr> {
    find_all(db)?;

    tracing::info!("===== =====\n");

    find_one(db)?;

    tracing::info!("===== =====\n");

    find_one_to_one(db)?;

    tracing::info!("===== =====\n");

    find_one_to_many(db)?;

    tracing::info!("===== =====\n");

    count_fruits_by_cake(db)?;

    tracing::info!("===== =====\n");

    find_many_to_many(db)?;

    if false {
        tracing::info!("===== =====\n");

        all_about_select_json(db)?;
    }

    tracing::info!("===== =====\n");

    let _a = find_all_stream(db);
    tracing::info!("{_a:?}");

    tracing::info!("===== =====\n");

    let _a = find_first_page(db);
    tracing::info!("{_a:?}");

    tracing::info!("===== =====\n");

    let _a = find_num_pages(db);
    tracing::info!("{_a:?}");

    Ok(())
}

fn find_all(db: &DbConn) -> Result<(), DbErr> {
    print!("find all cakes: ");

    let cakes: Vec<cake::Model> = Cake::find().all(db)?;

    tracing::info!("");
    for cc in cakes.iter() {
        tracing::info!("{cc:?}\n");
    }

    print!("find all fruits: ");

    let fruits = Fruit::find().all(db)?;

    tracing::info!("");
    for ff in fruits.iter() {
        tracing::info!("{ff:?}\n");
    }

    Ok(())
}

fn find_one_to_one(db: &DbConn) -> Result<(), DbErr> {
    print!("find fruits and cakes: ");

    let fruits_and_cakes: Vec<(fruit::Model, Option<cake::Model>)> =
        Fruit::find().find_also_related(Cake).all(db)?;

    tracing::info!("with loader: ");
    let fruits: Vec<fruit::Model> = Fruit::find().all(db)?;
    let cakes: Vec<Option<cake::Model>> = fruits.load_one(Cake, db)?;

    tracing::info!("");
    for (left, right) in fruits_and_cakes
        .into_iter()
        .zip(fruits.into_iter().zip(cakes.into_iter()))
    {
        tracing::info!("{left:?}");
        assert_eq!(left, right);
    }

    Ok(())
}

fn find_one_to_many(db: &DbConn) -> Result<(), DbErr> {
    print!("find cakes with fruits: ");

    let cakes_with_fruits: Vec<(cake::Model, Vec<fruit::Model>)> = Cake::find()
        .find_with_related(fruit::Entity)
        .all(db)
        ?;

    tracing::info!("with loader: ");
    let cakes: Vec<cake::Model> = Cake::find().all(db)?;
    let fruits: Vec<Vec<fruit::Model>> = cakes.load_many(Fruit, db)?;

    tracing::info!("");
    for (left, right) in cakes_with_fruits
        .into_iter()
        .zip(cakes.into_iter().zip(fruits.into_iter()))
    {
        tracing::info!("{left:?}\n");
        assert_eq!(left, right);
    }

    Ok(())
}

impl Cake {
    fn find_by_name(name: &str) -> Select<Self> {
        Self::find().filter(cake::Column::Name.contains(name))
    }
}

fn find_one(db: &DbConn) -> Result<(), DbErr> {
    print!("find one by primary key: ");

    let cheese: Option<cake::Model> = Cake::find_by_id(1).one(db)?;
    let Some(cheese) = cheese else {
        return Err(DbErr::Custom("not find".into()))
    };


    tracing::info!("");
    tracing::info!("{cheese:?}");
    tracing::info!("");

    print!("find one by name: ");

    let chocolate = Cake::find_by_name("chocolate").one(db)?;

    tracing::info!("");
    tracing::info!("{chocolate:?}");
    tracing::info!("");

    print!("find models belong to: ");

    let fruits = cheese.find_related(Fruit).all(db)?;

    tracing::info!("");
    for ff in fruits.iter() {
        tracing::info!("{ff:?}\n");
    }

    Ok(())
}

fn count_fruits_by_cake(db: &DbConn) -> Result<(), DbErr> {
    #[derive(Debug, FromQueryResult)]
    struct SelectResult {
        name: String,
        num_of_fruits: i32,
    }

    print!("count fruits by cake: ");

    let select = Cake::find()
        .left_join(Fruit)
        .select_only()
        .column(cake::Column::Name)
        .column_as(fruit::Column::Id.count(), "num_of_fruits")
        .group_by(cake::Column::Name);

    let results = select.into_model::<SelectResult>().all(db)?;

    tracing::info!("");
    for rr in results.iter() {
        tracing::info!("{rr:?}\n");
    }

    Ok(())
}

fn find_many_to_many(db: &DbConn) -> Result<(), DbErr> {
    print!("find cakes and fillings: ");

    let cakes_with_fillings: Vec<(cake::Model, Vec<filling::Model>)> =
        Cake::find().find_with_related(Filling).all(db)?;

    tracing::info!("with loader: ");
    let cakes: Vec<cake::Model> = Cake::find().all(db)?;
    let fillings: Vec<Vec<filling::Model>> =
        cakes.load_many_to_many(Filling, CakeFilling, db)?;

    tracing::info!("");
    for (left, right) in cakes_with_fillings
        .into_iter()
        .zip(cakes.into_iter().zip(fillings.into_iter()))
    {
        tracing::info!("{left:?}\n");
        assert_eq!(left, right);
    }

    print!("find fillings for cheese cake: ");

    let cheese = Cake::find_by_id(1).one(db)?;

    if let Some(cheese) = cheese {
        let fillings: Vec<filling::Model> = cheese.find_related(Filling).all(db)?;

        tracing::info!("");
        for ff in fillings.iter() {
            tracing::info!("{ff:?}\n");
        }
    }

    print!("find cakes for lemon: ");

    let lemon = Filling::find_by_id(2).one(db)?;

    if let Some(lemon) = lemon {
        let cakes: Vec<cake::Model> = lemon.find_related(Cake).all(db)?;

        tracing::info!("");
        for cc in cakes.iter() {
            tracing::info!("{cc:?}\n");
        }
    }

    Ok(())
}

fn all_about_select_json(db: &DbConn) -> Result<(), DbErr> {
    find_all_json(db)?;

    tracing::info!("===== =====\n");

    find_together_json(db)?;

    tracing::info!("===== =====\n");

    count_fruits_by_cake_json(db)?;

    Ok(())
}

fn find_all_json(db: &DbConn) -> Result<(), DbErr> {
    print!("find all cakes: ");

    let cakes = Cake::find().into_json().all(db)?;

    tracing::info!("\n{:?}\n", serde_json::to_string_pretty(&cakes));

    print!("find all fruits: ");

    let fruits = Fruit::find().into_json().all(db)?;

    tracing::info!("\n{:?}\n", serde_json::to_string_pretty(&fruits));

    Ok(())
}

fn find_together_json(db: &DbConn) -> Result<(), DbErr> {
    print!("find cakes and fruits: ");

    let cakes_fruits = Cake::find()
        .find_also_related(Fruit)
        .into_json()
        .all(db)
        ?;

    tracing::info!(
        "\n{:?}\n",
        serde_json::to_string_pretty(&cakes_fruits)
    );

    Ok(())
}

fn count_fruits_by_cake_json(db: &DbConn) -> Result<(), DbErr> {
    print!("count fruits by cake: ");

    let count = Cake::find()
        .left_join(Fruit)
        .select_only()
        .column(cake::Column::Name)
        .column_as(fruit::Column::Id.count(), "num_of_fruits")
        .group_by(cake::Column::Name)
        .into_json()
        .all(db)
        ?;

    tracing::info!("\n{:?}\n", serde_json::to_string_pretty(&count));

    Ok(())
}

fn find_all_stream(db: &DbConn) -> Result<(), DbErr> {
    use std::time::Duration;

    tracing::info!("find all cakes paginated: ");
    let mut cake_paginator = cake::Entity::find().paginate(db, 3);
    while let Some(cake_res) = cake_paginator.fetch_and_next()? {
        for cake in cake_res {
            tracing::info!("{cake:?}");
        }
    }

    tracing::info!("");
    tracing::info!("find all fruits paginated: ");
    let mut fruit_paginator = fruit::Entity::find().paginate(db, 3);
    while let Some(fruit_res) = fruit_paginator.fetch_and_next()? {
        for fruit in fruit_res {
            tracing::info!("{fruit:?}");
        }
    }

    tracing::info!("");
    tracing::info!("find all fruits with stream: ");
    let mut fruit_stream = fruit::Entity::find().paginate(db, 3).into_stream();
    while let Some(fruits) = fruit_stream.next() {
        for fruit in fruits {
            tracing::info!("{fruit:?}");
        }
        // sleep(Duration::from_millis(250));
    }

    tracing::info!("");
    tracing::info!("find all fruits in json with stream: ");
    let mut json_stream = fruit::Entity::find()
        .into_json()
        .paginate(db, 3)
        .into_stream();
    while let Some(jsons) = json_stream.next() {
        for json in jsons? {
            tracing::info!("{json:?}");
        }
        // sleep(Duration::from_millis(250));
    }

    Ok(())
}

fn find_first_page(db: &DbConn) -> Result<(), DbErr> {
    tracing::info!("fruits first page: ");
    let page = fruit::Entity::find().paginate(db, 3).fetch_page(0)?;
    for fruit in page {
        tracing::info!("{fruit:?}");
    }

    Ok(())
}

fn find_num_pages(db: &DbConn) -> Result<(), DbErr> {
    tracing::info!("fruits number of page: ");
    let num_pages = fruit::Entity::find().paginate(db, 3).num_pages()?;
    tracing::info!("{num_pages:?}");

    Ok(())
}
