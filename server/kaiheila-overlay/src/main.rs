#[macro_use]
extern crate log;

use actix_web::{middleware, web, App, HttpServer};
use actix_web_static_files;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    HttpServer::new(|| {
        let generated = generate();
        App::new()
            .wrap(middleware::Logger::default())
            .service(actix_web_static_files::ResourceFiles::new("/", generated))
    })
        .bind(("127.0.0.1", 8080))?
        .run()
        .await?;

    Ok(())
}
