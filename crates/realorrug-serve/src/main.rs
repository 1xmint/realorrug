// SPDX-License-Identifier: Apache-2.0
//! `realorrug-serve`: serves the public site's documents.

use std::net::SocketAddr;
use std::process::ExitCode;

/// Where to listen.
///
/// # A typo does not become a different address
///
/// A value that cannot be parsed refuses to start. Falling back to the default
/// would start a healthy process on a port nothing is pointed at, and the proxy
/// would answer 502 for a reason nothing on the box reports. Absence keeps the
/// default, since absence has an obvious right answer and a typo does not.
///
/// The default is not Radar's 8080, so both servers can run on one box.
///
/// # Errors
///
/// A message naming the value, for the operator who has to find the typo.
fn bind_address(configured: Option<&str>) -> Result<SocketAddr, String> {
    let Some(value) = configured else {
        return Ok(SocketAddr::from(([127, 0, 0, 1], 8090)));
    };
    value.parse().map_err(|e| {
        format!(
            "REALORRUG_BIND={value:?} is not an address ({e}); refusing to start on a port nobody asked for"
        )
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let bind = match bind_address(std::env::var("REALORRUG_BIND").ok().as_deref()) {
        Ok(bind) => bind,
        Err(why) => {
            eprintln!("realorrug-serve: {why}");
            return ExitCode::FAILURE;
        }
    };
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("realorrug-serve: cannot bind {bind}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("realorrug-serve listening on http://{bind}");
    // `into_make_service_with_connect_info`, not `into_make_service`: the
    // checker route's per-IP rate limit (design 0023 §4) needs the socket
    // peer address when `REALORRUG_TRUST_CLOUDFLARE` is unset -- without this the
    // route would fall back to a fixed placeholder for every visitor, and the
    // "10 a minute per IP" limit would not exist for anyone at all.
    let app = realorrug_serve::app().into_make_service_with_connect_info::<SocketAddr>();
    match axum::serve(listener, app).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("realorrug-serve: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bind_address;

    #[test]
    fn an_absent_variable_uses_the_documented_default() {
        assert_eq!(
            bind_address(None).expect("a default").to_string(),
            "127.0.0.1:8090"
        );
    }

    #[test]
    fn a_configured_address_is_used_as_written() {
        assert_eq!(
            bind_address(Some("127.0.0.1:8402"))
                .expect("an address")
                .to_string(),
            "127.0.0.1:8402"
        );
    }

    #[test]
    fn a_typo_refuses_to_start_rather_than_binding_somewhere_else() {
        for wrong in ["127.0.0.1;8402", "8402", "localhost:8402", ""] {
            let why = bind_address(Some(wrong)).expect_err("not an address");
            assert!(
                why.contains(wrong),
                "the message must name the value: {why}"
            );
        }
    }
}
