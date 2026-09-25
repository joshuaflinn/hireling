//! Tests for [`super`] — pure OIDC mechanics. The provider URLs are asserted
//! against the captured house contract
//! (`specs/003-authentik-oidc/contracts/oidc.md`), and the PKCE challenge
//! against the RFC 7636 test vector.

use chrono::Utc;
use cookie::Key;

use super::IdTokenClaims;
use super::ProviderUrls;
use super::Transaction;
use super::authorize_url;
use super::derive_provider_urls;
use super::map_claims;
use super::pkce_challenge;
use super::random_token;

fn key() -> Key {
    Key::derive_from(&[7_u8; 32])
}

#[test]
fn provider_urls_match_the_captured_house_contract() {
    let urls = derive_provider_urls("https://auth.flinntech.com/application/o/hireling/")
        .expect("fixture issuer is valid");
    assert_eq!(
        urls,
        ProviderUrls {
            authorize: "https://auth.flinntech.com/application/o/authorize/".to_owned(),
            token: "https://auth.flinntech.com/application/o/token/".to_owned(),
            jwks: "https://auth.flinntech.com/application/o/hireling/jwks/".to_owned(),
        },
        "the derived endpoints must equal the probed discovery document exactly"
    );
}

#[test]
fn an_issuer_without_a_trailing_path_is_rejected() {
    assert!(derive_provider_urls("auth.flinntech.com").is_err());
    assert!(derive_provider_urls("https://auth.flinntech.com").is_err());
    assert!(derive_provider_urls("https://auth.flinntech.com/application/o/hireling").is_err());
    assert!(derive_provider_urls("ftp://auth.flinntech.com/o/hireling/").is_err());
}

#[test]
fn pkce_challenge_matches_the_rfc_7636_test_vector() {
    // The S256 vector from RFC 7636 appendix B.
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        pkce_challenge(verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn random_tokens_are_43_chars_and_not_repeated() {
    let a = random_token().expect("token");
    let b = random_token().expect("token");
    assert_eq!(a.len(), 43, "256 bits base64url is 43 chars");
    assert_eq!(b.len(), 43);
    assert_ne!(a, b, "two draws colliding means the CSPRNG is broken");
}

#[test]
fn a_transaction_survives_an_encode_decode_round_trip() {
    let transaction = Transaction::generate().expect("generate");
    let value = transaction.encode(&key(), Utc::now()).expect("encode");
    let decoded = Transaction::decode(&value, &key(), Utc::now()).expect("decode");
    assert_eq!(decoded, transaction);
}

#[test]
fn a_tampered_transaction_cookie_is_rejected() {
    let transaction = Transaction::generate().expect("generate");
    let value = transaction.encode(&key(), Utc::now()).expect("encode");

    // Flip the payload: any edit invalidates the signature.
    let flipped = format!("{}B", value.get(..43).expect("43-char verifier"));
    assert!(
        Transaction::decode(&flipped, &key(), Utc::now()).is_none(),
        "a modified value must fail signature verification"
    );
    // A different key (what an attacker with another signing key has) fails.
    assert!(
        Transaction::decode(&value, &Key::derive_from(&[9_u8; 32]), Utc::now()).is_none(),
        "a cookie signed under another key must not verify"
    );
    // Garbage of every shape fails.
    assert!(Transaction::decode("", &key(), Utc::now()).is_none());
    assert!(Transaction::decode("not-a-signature", &key(), Utc::now()).is_none());
    assert!(Transaction::decode("AAAA_blob_value", &key(), Utc::now()).is_none());
}

#[test]
fn an_expired_transaction_is_rejected_even_with_a_valid_signature() {
    let transaction = Transaction::generate().expect("generate");
    let value = transaction.encode(&key(), Utc::now()).expect("encode");
    let past_the_ttl = Utc::now() + chrono::Duration::seconds(601);
    assert!(
        Transaction::decode(&value, &key(), past_the_ttl).is_none(),
        "past the 10-minute transaction TTL the login must start over"
    );
    let still_fresh = Utc::now() + chrono::Duration::seconds(599);
    assert!(Transaction::decode(&value, &key(), still_fresh).is_some());
}

#[test]
fn the_authorize_url_carries_the_full_code_flow_request() {
    let urls = ProviderUrls {
        authorize: "https://auth.test/o/authorize/".to_owned(),
        token: String::new(),
        jwks: String::new(),
    };
    let transaction = Transaction::generate().expect("generate");
    let url = authorize_url(
        &urls,
        "client-id",
        "https://app.example/api/auth/callback",
        &transaction,
    );

    assert!(url.starts_with("https://auth.test/o/authorize/?"));
    for piece in [
        "response_type=code",
        "scope=openid%20profile",
        "client_id=client-id",
        "redirect_uri=https%3A%2F%2Fapp.example%2Fapi%2Fauth%2Fcallback",
        "code_challenge_method=S256",
    ] {
        assert!(url.contains(piece), "authorize URL missing {piece}: {url}");
    }
    // state and nonce come from the transaction; the challenge is the S256
    // of its verifier.
    assert!(url.contains(&format!("state={}", transaction.state)));
    assert!(url.contains(&format!("nonce={}", transaction.nonce)));
    assert!(url.contains(&format!(
        "code_challenge={}",
        pkce_challenge(&transaction.code_verifier)
    )));
    assert!(
        !url.contains("code_verifier"),
        "the verifier never travels to the authorize URL"
    );
}

#[test]
fn claims_map_with_the_documented_fallbacks() {
    let full = IdTokenClaims {
        sub: "sub-1".to_owned(),
        nonce: None,
        preferred_username: Some("josh".to_owned()),
        name: Some("Josh".to_owned()),
    };
    let mapped = map_claims(&full, None);
    assert_eq!(mapped.sub, "sub-1");
    assert_eq!(mapped.username, "josh");
    assert_eq!(mapped.display_name, "Josh", "display_name prefers name");

    let no_name = IdTokenClaims {
        sub: "sub-1".to_owned(),
        nonce: None,
        preferred_username: Some("josh".to_owned()),
        name: None,
    };
    assert_eq!(
        map_claims(&no_name, None).display_name,
        "josh",
        "name falls back to username"
    );

    let bare = IdTokenClaims {
        sub: "sub-2".to_owned(),
        nonce: None,
        preferred_username: None,
        name: None,
    };
    assert_eq!(map_claims(&bare, Some("Becky")).username, "Becky");
    assert_eq!(map_claims(&bare, Some("Becky")).display_name, "Becky");
    assert_eq!(
        map_claims(&bare, None).username,
        "sub-2",
        "identity never depends on profile claims; the raw sub is the last fallback"
    );
}
