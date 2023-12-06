use std::net::TcpListener;
use once_cell::sync::Lazy;
use secrecy::ExposeSecret;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, get_configuration};
use zero2prod::startup::run;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});
pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}
async fn spawn_app() -> TestApp{
    Lazy::force(&TRACING);

    let mut configuration = get_configuration().expect("Failed to read configuration");
    let connection_pool= configure_database(&mut configuration.database).await;
    let lst = TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port");
    let port = lst.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}",port);
    let server = run(lst, connection_pool.clone()).expect("Failed to bind address");
    let _ = tokio::spawn(server);
    TestApp{
        address,
        db_pool: connection_pool,
    }
}

pub async fn configure_database(config: &mut DatabaseSettings) -> PgPool {
    println!("config db url: {}", &config.connection_string_without_db().expose_secret());
    let mut connection = PgConnection::connect(&config.connection_string().expose_secret())
        .await
        .expect("Failed to connect to Postgres");
    let new_db = Uuid::new_v4().to_string();
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, new_db).as_str())
        .await
        .expect("Failed to create database");

    config.database_name = new_db;
    let connection_pool = PgPool::connect(&config.connection_string().expose_secret())
        .await
        .expect("Failed to connect to Postgres");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");
    connection_pool
}
#[tokio::test]
async fn health_check_works() {
    let app= spawn_app().await;
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request");
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_return_a_200_for_valid_form_data() {
    let app = spawn_app().await;

    let client = reqwest::Client::new();
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = client
        .post(&format!("{}/subscriptions", &app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");
    assert!(response.status().is_success());
    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch save subscription");
}

#[tokio::test]
async fn subscribe_return_a_400_when_data_is_missing() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let tests = vec![
        ("email=ursula_le_guin%40gmail.com", "missing name"),
        ("name=le%20guin", "missing email"),
        ("", "missing name and email"),
    ];
    for (invalid_body, error_message) in tests {
        let response = client
            .post(&format!("{}/subscriptions", &app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(
            400,
            response.status().as_u16(),
            "The api did not fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}
