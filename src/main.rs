#[allow(unused_imports)] use crate::prelude::*;

use actix_web::{middleware, get, web, App, HttpServer, Responder};

mod archive;
mod async_util;
mod filelock;
mod prelude;
mod ship;
mod runtime;

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::MergeOnly,
            ))
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .service(greet)
    }).bind(("127.0.0.1", 8050))?.run().await
}