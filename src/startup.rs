use crate::configuration::{Settings, DatabaseSettings};
use crate::email_client::EmailClient;
use crate::routes::{health_check, subscribe, confirm};
use actix_web::dev::Server;
use actix_web::{web, App, HttpRequest, HttpServer, Responder};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        let connection_pool = PgPoolOptions::new()
            .connect_timeout(std::time::Duration::from_secs(2))
            .connect_lazy(&configuration.database.connection_string().expose_secret())
            .expect("Failed to connect to Postgres");

        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender email address");
        let email_client = EmailClient::new(
            configuration.email_client.base_url.clone(),
            sender_email,
            configuration.email_client.authorization_token.clone(),
            configuration.email_client.timeout(),
        );

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let lst = TcpListener::bind(address).expect("Failed to bind port");
        let port = lst.local_addr().unwrap().port();
        let server = run(lst, connection_pool, email_client, configuration.application.base_url)?;
        Ok(Self{port, server})
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub struct ApplicationUrl(pub String);

pub fn run(
    lst: TcpListener,
    connection: PgPool,
    email_client: EmailClient,
    base_url: String,
) -> Result<Server, std::io::Error> {
    let connection = web::Data::new(connection);
    let email_client = web::Data::new(email_client);
    let base_url = web::Data::new(ApplicationUrl(base_url));

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/", web::get().to(greet))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .route("/health_check", web::get().to(health_check))
            .app_data(connection.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
    })
    .listen(lst)?
    .run();
    Ok(server)
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
        PgPoolOptions::new()
            .connect_timeout(std::time::Duration::from_secs(2))
            .connect_lazy_with(configuration.with_db())
} 

async fn greet(req: HttpRequest) -> impl Responder {
    let name = req.match_info().get("name").unwrap_or("World");
    format!("Hello {}!", &name)
}
