#[macro_use] extern crate log;

use actix_web::{App, HttpServer, middleware, web};

mod client;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
    })
        .bind(("127.0.0.1", 8080))?
        .run()
        .await?;

    Ok(())
}