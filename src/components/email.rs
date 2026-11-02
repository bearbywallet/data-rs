use lettre::{
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use log::{error, info};

pub struct Emailer {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: Mailbox,
    endpoint: String,
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
                endpoint: String::new(),
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
            // Mechanism order matters: lettre picks the FIRST mechanism that the
            // server advertises (get_auth_mechanism) and never falls back after a
            // failure. LOGIN first keeps OpenSMTPD working (it rejects AUTH PLAIN
            // with an initial response, 501) and Gmail; PLAIN is then selected only
            // by servers that do not advertise LOGIN (previously a hard failure).
            builder
                .credentials(Credentials::new(user, pass))
                .authentication(vec![Mechanism::Login, Mechanism::Plain])
        };

        info!("SMTP: enabled {}:{}", host, port);

        Emailer {
            mailer: Some(builder.build()),
            from,
            endpoint: format!("{host}:{port}"),
        }
    }

    /// Send the verification code. Returns true only when the message was
    /// accepted by the SMTP relay (i.e. a real send — delivery/receipt is
    /// not tracked and is a separate concern).
    pub async fn send_link(&self, to: &str, url: &str) -> bool {
        let Some(mailer) = &self.mailer else {
            return false;
        };

        let to_mailbox: Mailbox = match to.parse() {
            Ok(m) => m,
            Err(_) => {
                error!("SMTP: incorrect destination address {to}");
                return false;
            }
        };

        let email = match Message::builder()
            .from(self.from.clone())
            .to(to_mailbox)
            .subject("Confirm your Bearby Card subscription")
            .multipart(MultiPart::alternative_plain_html(
                link_plain(url),
                link_html(url),
            )) {
            Ok(e) => e,
            Err(e) => {
                error!("SMTP [{}]: build to {to} failed: {e}", self.endpoint);
                return false;
            }
        };

        match mailer.send(email).await {
            Ok(_) => {
                info!("SMTP: confirmation link sent to {to}");
                true
            }
            Err(e) => {
                error!("SMTP [{}]: send to {to} failed: {e}", self.endpoint);
                false
            }
        }
    }

    pub async fn send_welcome(&self, to: &str, unsub_url: &str, privacy_url: &str) {
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
            .multipart(MultiPart::alternative_plain_html(
                welcome_plain(unsub_url),
                welcome_html(unsub_url, privacy_url),
            )) {
            Ok(e) => e,
            Err(e) => {
                error!("SMTP [{}]: build to {to} failed: {e}", self.endpoint);
                return;
            }
        };

        match mailer.send(email).await {
            Ok(_) => info!("SMTP: welcome sent to {to}"),
            Err(e) => error!("SMTP [{}]: send to {to} failed: {e}", self.endpoint),
        }
    }
}

fn link_html(url: &str) -> String {
    format!(
        r#"<!doctype html>
<html><body style="margin:0;padding:0;background:#0b0910;font-family:Arial,Helvetica,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#0b0910;padding:48px 16px;">
<tr><td align="center">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;background:#17131f;border:1px solid #2a2338;border-radius:16px;padding:40px;">
<tr><td style="color:#ac59ff;font-size:12px;font-weight:bold;letter-spacing:2px;text-transform:uppercase;">Bearby Card</td></tr>
<tr><td style="color:#ffffff;font-size:24px;font-weight:bold;padding:14px 0 8px;">Confirm your email</td></tr>
<tr><td style="color:#a89fb8;font-size:15px;line-height:1.6;padding-bottom:24px;">Click the button below to finish joining the waitlist. The link is valid for 15 minutes.</td></tr>
<tr><td align="center" style="padding:8px 0 24px;"><a href="{url}" style="display:inline-block;padding:16px 36px;background:#ac59ff;border-radius:12px;text-decoration:none;"><span style="font-family:Arial,Helvetica,sans-serif;font-size:16px;font-weight:bold;color:#ffffff;">Confirm subscription</span></a></td></tr>
<tr><td style="color:#a89fb8;font-size:13px;line-height:1.6;padding-top:6px;">Button not working? Paste this link into your browser:<br><a href="{url}" style="color:#ac59ff;word-break:break-all;">{url}</a></td></tr>
<tr><td style="color:#6f6880;font-size:13px;padding-top:18px;">If you didn&apos;t request this, just ignore this email.</td></tr>
</table>
</td></tr>
</table>
</body></html>"#,
        url = url
    )
}

fn link_plain(url: &str) -> String {
    format!(
        "Confirm your Bearby Card subscription:\n\n{url}\n\nOpen this link to finish joining the waitlist. The link is valid for 15 minutes.\n\nIf you didn't request this, just ignore this email.\n"
    )
}

fn welcome_plain(unsub_url: &str) -> String {
    format!(
        "You're on the waitlist!\n\nThanks for your interest in the Bearby Card. We'll email you as soon as it becomes available.\n\nWe use your email and country of residence to communicate about Bearby Card availability and plan card support. See our Privacy Policy: https://bearby.io/privacy-policy#card-waitlist\n\nUnsubscribe: {unsub_url}\n"
    )
}

fn welcome_html(unsub_url: &str, privacy_url: &str) -> String {
    format!(
        r#"<!doctype html>
<html><body style="margin:0;padding:0;background:#0b0910;font-family:Arial,Helvetica,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#0b0910;padding:48px 16px;">
<tr><td align="center">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;background:#17131f;border:1px solid #2a2338;border-radius:16px;padding:40px;">
<tr><td style="color:#ac59ff;font-size:12px;font-weight:bold;letter-spacing:2px;text-transform:uppercase;">Bearby Card</td></tr>
<tr><td style="color:#ffffff;font-size:24px;font-weight:bold;padding:14px 0 8px;">You&#39;re on the waitlist!</td></tr>
<tr><td style="color:#a89fb8;font-size:15px;line-height:1.6;padding-bottom:12px;">Thanks for your interest in the Bearby Card. We&#39;ll email you as soon as it becomes available.</td></tr>
<tr><td style="color:#a89fb8;font-size:15px;line-height:1.6;padding-bottom:28px;">We use your email and country of residence to communicate about Bearby Card availability and plan card support. See our <a href="{privacy}" style="color:#ac59ff;">Privacy Policy</a>.</td></tr>
<tr><td style="border-top:1px solid #2a2338;padding-top:20px;">
<a href="{unsub}" style="color:#8f86a0;font-size:13px;">Unsubscribe</a>
</td></tr>
</table>
</td></tr>
</table>
</body></html>"#,
        unsub = unsub_url,
        privacy = privacy_url
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_html_renders_url_as_href_and_text() {
        let url = "https://bearby.io/waitlist/confirmed?email=a%40b.co&code=123456";
        let html = link_html(url);
        assert!(html.contains(&format!("href=\"{url}\"")));
        assert!(html.contains("Confirm subscription"));
    }

    #[test]
    fn link_plain_contains_url() {
        let url = "https://bearby.io/waitlist/confirmed?email=a%40b.co&code=123456";
        assert!(link_plain(url).contains(url));
    }
}
