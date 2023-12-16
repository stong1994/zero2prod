use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::spawn_app;

#[tokio::test]
async fn subscribe_return_a_200_for_valid_form_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    let response = app.post_subscription(body.to_string()).await;
    assert!(response.status().is_success());
    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persists_the_new_subscriber() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscription(body.to_string()).await;

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch save subscription");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
}
#[tokio::test]
async fn subscribe_return_a_400_when_data_is_missing() {
    let app = spawn_app().await;
    let tests = vec![
        ("email=ursula_le_guin%40gmail.com", "missing name"),
        ("name=le%20guin", "missing email"),
        ("", "missing name and email"),
    ];
    for (invalid_body, error_message) in tests {
        let response = app.post_subscription(invalid_body.to_string()).await;
        assert_eq!(
            400,
            response.status().as_u16(),
            "The api did not fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_return_a_400_when_fields_are_present_but_invalid() {
    let app = spawn_app().await;
    let tests = vec![("name=&email=ursula_le_guin%40gmail.com", "missing name")];
    for (invalid_body, error_message) in tests {
        let response = app.post_subscription(invalid_body.to_string()).await;
        assert_eq!(
            400,
            response.status().as_u16(),
            "The api did not fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_send_confirm_email_for_valid_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    app.post_subscription(body.into()).await;
}

#[tokio::test]
async fn subscribe_sends_a_conformation_email_with_a_link() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;
    app.post_subscription(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let confirmation_links = app.get_confirmation_links(email_request);
    assert_eq!(confirmation_links.html, confirmation_links.plain_text);
}

#[tokio::test]
async fn subscribe_fails_if_there_is_a_fatal_database_error() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    sqlx::query!("ALTER TABLE subscription_tokens DROP COLUMN subscription_token;")
        .execute(&app.db_pool)
        .await
        .unwrap();

    let response = app.post_subscription(body.to_string()).await;
    assert_eq!(response.status().as_u16(), 500);
}
