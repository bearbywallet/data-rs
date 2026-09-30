use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use log::{error, info};

pub struct Emailer {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: Mailbox,
}

impl Emailer {
    /// SMTP is optional: without SMTP_HOST emails are skipped (joining still works).
    /// Port 465 => implicit TLS, otherwise STARTTLS.
    pub fn new() -> Self {
        let from: Mailbox = std::env::var("SMTP_FROM")
            .unwrap_or_else(|_| "Bearby <noreply@bearby.io>".to_string())
            .parse()
            .expect("Incorrect SMTP_FROM");

        let host = std::env::var("SMTP_HOST").unwrap_or_default();

        if host.is_empty() {
            info!("SMTP: SMTP_HOST is not set, waitlist emails disabled");
            return Emailer {
                mailer: None,
                from,
            };
        }

        let port: u16 = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "465".to_string())
            .parse()
            .expect("SMTP_PORT should be u16");
        let user = std::env::var("SMTP_USER").unwrap_or_default();
        let pass = std::env::var("SMTP_PASS").unwrap_or_default();

        let builder = if port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
        }
        .expect("Cannot create SMTP transport")
        .port(port);

        let builder = if user.is_empty() {
            builder
        } else {
            builder.credentials(Credentials::new(user, pass))
        };

        info!("SMTP: enabled {}:{}", host, port);

        Emailer {
            mailer: Some(builder.build()),
            from,
        }
    }

    pub async fn send_welcome(&self, to: &str, unsub_url: &str) {
        let Some(mailer) = &self.mailer else {
            return;
        };

        let to_mailbox: Mailbox = match to.parse() {
            Ok(m) => m,
            Err(_) => {
                error!("SMTP: incorrect destination address {to}");
                return;
            }
        };

        let email = match Message::builder()
            .from(self.from.clone())
            .to(to_mailbox)
            .subject("You're on the Bearby Card waitlist!")
            .header(ContentType::TEXT_HTML)
            .body(welcome_html(unsub_url))
        {
            Ok(e) => e,
            Err(e) => {
                error!("SMTP: build error {e}");
                return;
            }
        };

        match mailer.send(email).await {
            Ok(_) => info!("SMTP: welcome sent to {to}"),
            Err(e) => error!("SMTP: send error {e}"),
        }
    }
}

fn welcome_html(unsub_url: &str) -> String {
    format!(
        r#"<!doctype html>
<html><body style="margin:0;padding:0;background:#0b0910;font-family:Arial,Helvetica,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#0b0910;padding:48px 16px;">
<tr><td align="center">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;background:#17131f;border:1px solid #2a2338;border-radius:16px;padding:40px;">
<tr><td style="color:#ac59ff;font-size:12px;font-weight:bold;letter-spacing:2px;text-transform:uppercase;">Bearby Card</td></tr>
<tr><td style="color:#ffffff;font-size:24px;font-weight:bold;padding:14px 0 8px;">You&#39;re on the waitlist!</td></tr>
<tr><td style="color:#a89fb8;font-size:15px;line-height:1.6;padding-bottom:28px;">Thanks for your interest in the Bearby Card. We&#39;ll email you as soon as it becomes available.</td></tr>
<tr><td style="border-top:1px solid #2a2338;padding-top:20px;">
<a href="{unsub}" style="color:#8f86a0;font-size:13px;">Unsubscribe</a>
</td></tr>
</table>
</td></tr>
</table>
</body></html>"#,
        unsub = unsub_url
    )
}
