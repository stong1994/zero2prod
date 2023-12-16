use crate::{routes::error_chain_fmt, email_client::{EmailClient}, domain::SubscriberEmail};
use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Context;
use reqwest::StatusCode;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}
#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            PublishError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
pub async fn publish_newsletter(
     body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
     email_client: web::Data<EmailClient>,
    ) -> Result<HttpResponse, PublishError> {
    let subscribers = get_confirmed_subscribers(&pool).await?;
    for subscriber in subscribers {
        email_client.send_email(
            &subscriber.email, 
            &body.title,
             &body.content.html,
              &body.content.text,
            )
            .await
            .with_context(||{
                format!("Failed to send newsletter to {}", &subscriber.email.0)
            })?;
    }

    Ok(HttpResponse::Ok().finish())
}

pub struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Result<Vec<ConfirmedSubscriber>, anyhow::Error> {
    struct Row {
        email: String,
    }
    let rows:  Vec<Row> = sqlx::query_as!(
        Row,
        r#"
SELECT email
FROM subscriptions
WHERE status = 'confirmed'
"#,
    )
    .fetch_all(pool)
    .await?;

    let confirmed_subscribes = rows
    .into_iter()
    .filter_map(|r| match SubscriberEmail::parse(r.email){
        Ok(email) => Some(ConfirmedSubscriber{email}),
        Err(error) => {
            tracing::warn!(
                "A confirmed subscriber is using an invalid email address.\n{}.",
error
            );
            None
        }
    })
    .collect();
    
    Ok(confirmed_subscribes)
}
