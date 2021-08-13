#[macro_use]
extern crate diesel;
extern crate dotenv;
#[macro_use]
extern crate rocket;
extern crate blake2;
extern crate hex;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate rocket_sync_db_pools;

mod auth;
mod models;
mod schema;

use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use rocket::form::Form;
use rocket::http::Cookie;
use rocket::http::CookieJar;
use rocket::http::Status;
use rocket::response::content;

use auth::random_id;
use auth::DbConn;
use auth::UserAuth;
use models::Commitment;
use models::Initiative;
use models::InitiativeSupport;
use models::UserLogin;

use models::User;

#[post("/user_new", data = "<user>")]
async fn user_new(user: Form<User>, conn: DbConn, cookies: &CookieJar<'_>) -> Status {
    while {
        let mut new_user = user.clone();
        new_user.name = random_id(&user.name);
        new_user.hash_password();

        let count = conn
            .run(move |c| {
                use schema::users::dsl::*;

                diesel::insert_or_ignore_into(users)
                    .values(&new_user)
                    .execute(c)
                    .unwrap()
            })
            .await;

        count == 0
    } {}

    cookies.add_private(Cookie::new("name", user.name.clone()));
    Status::Created
}

#[post("/user_edit", data = "<user>")]
async fn user_edit(auth: UserAuth, conn: DbConn, mut user: Form<User>) -> Status {
    if auth.0 != user.name {
        return Status::Unauthorized;
    }
    user.hash_password();

    conn.run(move |c| {
        diesel::update(&*user).set(&*user).execute(c).unwrap();
    })
    .await;

    Status::Ok
}

#[post("/user_login", data = "<user>")]
async fn user_login(mut user: Form<UserLogin>, conn: DbConn, cookies: &CookieJar<'_>) -> Status {
    user.hash_password();

    let user_name = user.name.clone();
    let db_user = conn
        .run(move |c| {
            use schema::users::dsl::*;
            users
                .find(&user_name)
                .get_result::<User>(c)
                .optional()
                .unwrap()
        })
        .await;

    if let Some(db_user) = db_user {
        if user.password == db_user.password {
            cookies.add_private(Cookie::new("name", user.name.clone()));
            Status::Accepted
        } else {
            Status::NotAcceptable
        }
    } else {
        Status::NotFound
    }
}

#[post("/user_logout")]
fn user_logout(cookies: &CookieJar<'_>) -> Status {
    cookies.remove_private(Cookie::named("name"));
    Status::Ok
}

#[get("/user/<user_name>")]
async fn user(
    _auth: UserAuth,
    conn: DbConn,
    user_name: String,
) -> Result<content::Json<String>, Status> {
    let item = conn
        .run(move |c| {
            use schema::users::dsl::*;
            users.find(&user_name).get_result::<User>(c)
        })
        .await;

    if let Ok(user) = item {
        Ok(content::Json(serde_json::to_string(&user).unwrap()))
    } else {
        Err(Status::NotFound)
    }
}

#[post("/commitment_new", data = "<commitment>")]
async fn commitment_new(_auth: UserAuth, conn: DbConn, commitment: Form<Commitment>) -> Status {
    while {
        let mut new_commitment = commitment.clone();
        new_commitment.name = random_id(&commitment.name);

        let count = conn
            .run(move |c| {
                use schema::commitments::dsl::*;
                diesel::insert_or_ignore_into(commitments)
                    .values(&new_commitment)
                    .execute(c)
                    .unwrap()
            })
            .await;

        count == 0
    } {}

    Status::Created
}

#[get("/commitment/<commitment_name>")]
async fn commitment(
    _auth: UserAuth,
    conn: DbConn,
    commitment_name: String,
) -> Result<content::Json<String>, Status> {
    let item = conn
        .run(move |c| {
            use schema::commitments::dsl::*;
            commitments
                .find(&commitment_name)
                .get_result::<Commitment>(c)
        })
        .await;

    if let Ok(commitment) = item {
        Ok(content::Json(serde_json::to_string(&commitment).unwrap()))
    } else {
        Err(Status::NotFound)
    }
}

#[post("/initiative_new", data = "<initiative>")]
async fn initiative_new(auth: UserAuth, conn: DbConn, initiative: Form<Initiative>) -> Status {
    if initiative.user.is_some() && initiative.user.as_ref().unwrap() != &auth.0 {
        return Status::Unauthorized;
    }

    while {
        let mut new_initiative = initiative.clone();
        new_initiative.name = random_id(&initiative.name);

        let count = conn
            .run(move |c| {
                use schema::initiatives::dsl::*;
                diesel::insert_or_ignore_into(initiatives)
                    .values(&new_initiative)
                    .execute(c)
                    .unwrap()
            })
            .await;

        count == 0
    } {}

    let support = InitiativeSupport {
        initiative_commitment: initiative.commitment.clone(),
        initiative_name: initiative.name.clone(),
    };

    initiative_support_add(auth, conn, Form::from(support)).await;

    Status::Created
}

#[get("/initiative/<commitment_name>/<initiative_name>")]
async fn initiative(
    _auth: UserAuth,
    conn: DbConn,
    commitment_name: String,
    initiative_name: String,
) -> Result<content::Json<String>, Status> {
    let item = conn
        .run(move |c| {
            use schema::initiatives::dsl::*;
            initiatives
                .find((commitment_name, initiative_name))
                .get_result::<Initiative>(c)
        })
        .await;

    if let Ok(initiative) = item {
        Ok(content::Json(serde_json::to_string(&initiative).unwrap()))
    } else {
        Err(Status::NotFound)
    }
}

// #[post("/initiative_edit", data = "<initiative>")]
// fn initiative_edit(auth: UserAuth, initiative: Form<Initiative>) -> Status {
//     let conn = establish_connection();

//     let count = diesel::update(&*initiative)
//         .set(&*initiative)
//         .execute(&conn)
//         .unwrap();

//     if count == 1 {
//         Status::Ok
//     } else {
//         Status::NotFound
//     }
// }

#[post("/initiative_support_add", data = "<support>")]
async fn initiative_support_add(auth: UserAuth, conn: DbConn, support: Form<InitiativeSupport>) {
    conn.run(move |c| {
        use schema::initiative_supports::dsl::*;
        diesel::insert_or_ignore_into(initiative_supports)
            .values((user.eq(&auth.0), &*support))
            .execute(c)
            .unwrap()
    })
    .await;
}

#[post("/initiative_support_remove", data = "<support>")]
async fn initiative_support_remove(auth: UserAuth, conn: DbConn, support: Form<InitiativeSupport>) {
    conn.run(move |c| {
        use schema::initiative_supports::dsl::*;
        diesel::delete(initiative_supports)
            .filter(user.eq(&auth.0))
            .filter(initiative_commitment.eq(&support.initiative_commitment))
            .filter(initiative_name.eq(&support.initiative_name))
            .execute(c)
            .unwrap()
    })
    .await;
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount(
            "/",
            routes![
                user_new,
                user_edit,
                user_login,
                user_logout,
                user,
                commitment_new,
                commitment,
                initiative_new,
                initiative,
                initiative_support_add,
                initiative_support_remove
            ],
        )
        .attach(DbConn::fairing())
}
