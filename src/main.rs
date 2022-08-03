#[allow(unused_imports)] use crate::prelude::*;

use actix_web::{middleware, get, post, web, App, HttpServer, Responder};
use actix_multipart::Multipart;
// use std::sync::RwLock;

mod archive;
mod async_util;
mod filelock;
mod net_util;
// mod patp;
mod prelude;
mod runtime;
mod ship;
mod util;

struct AppState {
    off: Vec<ship::PierState>,
    on: Vec<ship::Ship>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "method")]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
enum PostPierForm {
    FromKeyfile {
        name: String,
    },
    FromPierArchive {
    }
}

#[post("/pier")]
async fn greet(form: web::Json<PostPierForm>, payload: Multipart) -> impl Responder {
    format!("Hello!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // let ship
    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::MergeOnly,
            ))
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .service(greet)
    }).bind(("127.0.0.1", 8000))?.run().await
}