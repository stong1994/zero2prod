use std::net::TcpListener;
use sqlx::{PgPool};
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;
use tracing_log::LogTracer;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    LogTracer::init().expect("Failed to set logger");
    let subscriber = get_subscriber("zero2prod".into(), "info".into());
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("Failed to connect to Postgres");
    let address = format!("127.0.0.1:{}", configuration.application_port);
    let lst = TcpListener::bind(address)
        .expect("Failed to bind port");
    run(lst, connection_pool)?
        .await
}
