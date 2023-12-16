use once_cell::sync::Lazy;
use secrecy::ExposeSecret;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::email_client::EmailClient;
use zero2prod::startup::{get_connection_pool, run, Application};
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

pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub plain_text: reqwest::Url,
}

pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub db_pool: PgPool,
    pub email_server: MockServer,
}

impl TestApp {
    //
    pub async fn post_subscription(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(email_request.body.as_ref()).unwrap();

        let get_link = |s| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();
            assert_eq!(links.len(), 1);
            let link = links[0].as_str().to_owned();
            let mut link = reqwest::Url::parse(&link).unwrap();
            assert_eq!(link.host_str().unwrap(), "127.0.0.1");
            link.set_port(Some(self.port)).unwrap();
            link
        };
        let html_link = get_link(&body["HtmlBody"].as_str().unwrap());
        let plain_text_link = get_link(&body["TextBody"].as_str().unwrap());
        ConfirmationLinks { html: html_link, plain_text: plain_text_link }
    }

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
        .post(&format!("{}/newsletters", &self.address))
        .json(&body)
        .send()
        .await
        .expect("Failed to execute request.")
    }  
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let email_server = MockServer::start().await;
    let mut configuration = {
        let mut c = get_configuration().expect("Failed to read configuration");
        // c.database.database_name = Uuid::new_v4().to_string();
        c.application.port = 0;
        c.email_client.base_url = email_server.uri();
        c
    };

    let db_pool = configure_database(&mut configuration.database).await;

    let application: Application =
        Application::build(configuration.clone()).expect("Failed to build application");
    let port = application.port();
    let address = format!("http://127.0.0.1:{}", port);

    let _ = tokio::spawn(application.run_until_stopped());
    TestApp {
        address,
        port,
        db_pool,
        email_server,
    }
}

pub async fn configure_database(config: &mut DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    let new_db_name = Uuid::new_v4().to_string();
    config.database_name = new_db_name.clone().to_string();
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, &new_db_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");
    connection_pool
}
