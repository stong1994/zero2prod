use std::net::TcpListener;
use actix_web::{App, HttpRequest, HttpServer, Responder, web};
use actix_web::dev::Server;
use actix_web::middleware::Logger;
use sqlx::{PgPool};
use crate::routes::{health_check, subscribe};

pub fn run(
    lst: TcpListener,
    connection: PgPool
) -> Result<Server, std::io::Error> {
    let connection = web::Data::new(connection);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .route("/", web::get().to(greet))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/health_check", web::get().to(health_check))
            .app_data(connection.clone())
    })
        .listen(lst)?
        .run();
    Ok(server)
}

async fn greet(req: HttpRequest) -> impl Responder{
    let name = req.match_info().get("name").unwrap_or("World");
    format!("Hello {}!", &name)
}
