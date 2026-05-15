use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::config::Email as EmailConfig;

async fn send_email(
    cfg: &EmailConfig,
    to_email: &str,
    subject: &str,
    body_text: String,
) -> Result<(), String> {
    if cfg.smtp_host.is_empty() {
        tracing::warn!(to = to_email, "{}: {}", subject, body_text);
        return Ok(());
    }

    let from = format!("{} <{}>", cfg.from_name, cfg.from_email)
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;
    let to = to_email
        .parse()
        .map_err(|e| format!("invalid to address: {e}"))?;

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body_text)
        .map_err(|e| format!("failed to build email: {e}"))?;

    let creds = Credentials::new(cfg.smtp_username.clone(), cfg.smtp_password.clone());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp_host)
        .map_err(|e| format!("SMTP relay error: {e}"))?
        .port(cfg.smtp_port)
        .credentials(creds)
        .build();

    mailer.send(email).await.map_err(|e| format!("email send failed: {e}"))?;
    Ok(())
}

pub async fn send_verification_email(
    cfg: &EmailConfig,
    to_email: &str,
    verification_url: &str,
) -> Result<(), String> {
    send_email(
        cfg,
        to_email,
        "Confirm your email address",
        format!(
            "Click the link below to confirm your email address:\n\n\
             {verification_url}\n\n\
             This link expires in 24 hours."
        ),
    )
    .await
}

pub async fn send_password_reset_email(
    cfg: &EmailConfig,
    to_email: &str,
    reset_url: &str,
) -> Result<(), String> {
    send_email(
        cfg,
        to_email,
        "Reset your password",
        format!(
            "Click the link below to reset your password:\n\n\
             {reset_url}\n\n\
             This link expires in 1 hour. If you did not request a reset, ignore this email."
        ),
    )
    .await
}
