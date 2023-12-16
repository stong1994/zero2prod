use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient, startup::ApplicationUrl,
};
use actix_web::{web, HttpResponse, Responder, ResponseError};
use anyhow::Context;
use chrono::Utc;
use rand::{thread_rng, Rng, distributions::Alphanumeric};
use reqwest::StatusCode;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("0")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<String> for SubscribeError {
    fn from(e: String) -> Self {
        Self::ValidationError(e)
    }
}

#[derive(serde::Deserialize)]
pub struct SubscribeData {
    email: String,
    name: String,
}

impl TryFrom<SubscribeData> for NewSubscriber {
    type Error = String;
    fn try_from(value: SubscribeData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { email, name })
    }
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<SubscribeData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationUrl>,
) -> Result<HttpResponse, SubscribeError>{
    let mut transaction = pool
    .begin()
    .await
    .context("Failed to acquire a Postgre connection from the pool.")?;


    let subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let subscriber_id= insert_subscriber(&subscriber, &mut transaction)
    .await
    .context("Failed to insert new subscriber in the database")?;
    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token)
    .await
    .context("Failed to store the confirmation token for a new subscriber")?;
    transaction
    .commit()
    .await
    .context("Failed to commit transaction to store a subscriber.")?;
    send_confirmation_email(&email_client, subscriber, &base_url.0, &subscription_token)
    .await
    .context("Failed to send a confirmation email")?;
    Ok(HttpResponse::Ok().finish())
}

// #[derive(Debug)]
pub struct StoreTokenError(sqlx::Error);

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // write!(f, "{}\nCaused by:\n\t{}", self, self.0)
        error_chain_fmt(self, f)
    }
}
impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while \
trying to store a subscription token."
        )
    }
}

impl actix_web::ResponseError for StoreTokenError {}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[tracing::instrument(
    name = "Storing token",
    skip(transaction, subscription_token)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError>{
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
        )
        .execute(transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            StoreTokenError(e)
        })?;
        Ok(())
}

#[tracing::instrument(
    name = "Sending confirmation email",
    skip(email_client, subscriber, base_url, subscription_token)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!("{}/subscriptions/confirm?subscription_token={}", base_url, subscription_token);
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        lick <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(&subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}


#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    new_subscriber: &NewSubscriber,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions(id, email, name, subscribed_at, status)
        VALUES($1, $2, $3, $4, $5)
        "#,
        id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now(),
        "pending_confirmation",
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(id)
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
    .map(char::from)
    .take(25)
    .collect()
}

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}