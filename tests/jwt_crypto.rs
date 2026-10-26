//! All tokens and PEM keys in these tests are synthetic, disposable fixtures.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use devshop::jwt::{self, AlgorithmChoice as Alg, KeyEncoding as Key};
use serde_json::json;

const PUBLIC_EXAMPLE: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

fn unsigned_fixture(header: &str, payload: &str) -> String {
    format!(
        "{}.{}.AA",
        URL_SAFE_NO_PAD.encode(header),
        URL_SAFE_NO_PAD.encode(payload)
    )
}

#[test]
fn known_hmac_vector_and_wrong_key() {
    let decoded = jwt::decode(PUBLIC_EXAMPLE).unwrap();
    assert_eq!(decoded.payload["name"], "John Doe");
    assert!(
        jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Hs256, "your-256-bit-secret", Key::Text)
            .unwrap()
    );
    assert!(
        !jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Hs256, "a-different-key", Key::Text).unwrap()
    );
    assert_eq!(
        jwt::sign(
            &decoded.header_pretty,
            &decoded.payload_pretty,
            Alg::Hs256,
            "your-256-bit-secret",
            Key::Text
        )
        .unwrap(),
        PUBLIC_EXAMPLE
    );
}

#[test]
fn all_hmac_algorithms_and_base64_keys() {
    for algorithm in [Alg::Hs256, Alg::Hs384, Alg::Hs512] {
        let header = json!({"alg": algorithm.name(), "typ": "JWT"}).to_string();
        let token = jwt::sign(
            &header,
            r#"{"sub":"test","exp":1}"#,
            algorithm,
            "synthetic-secret",
            Key::Text,
        )
        .unwrap();
        assert!(
            jwt::verify_signature(&token, algorithm, "c3ludGhldGljLXNlY3JldA==", Key::Base64)
                .unwrap()
        );
        // Expiration never changes the cryptographic signature result.
        assert_eq!(
            jwt::claim_times(&jwt::decode(&token).unwrap().payload, 10)[0].status,
            jwt::TimeStatus::Expired
        );
    }
}

#[test]
fn refuses_algorithm_confusion_and_pem_hmac_secret() {
    assert!(
        jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Rs256, "anything", Key::Text)
            .unwrap_err()
            .to_string()
            .contains("算法不匹配")
    );
    assert!(jwt::sign(r#"{"alg":"HS256"}"#, "{}", Alg::Hs512, "secret", Key::Text).is_err());
    let pem = include_str!("jwt_fixtures/rsa-public.pem");
    assert!(
        jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Hs256, pem, Key::Text)
            .unwrap_err()
            .to_string()
            .contains("不接受 PEM")
    );
    assert!(jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Hs256, "", Key::Text).is_err());
    assert!(jwt::verify_signature(PUBLIC_EXAMPLE, Alg::Hs256, "%%%", Key::Base64).is_err());
}

#[test]
fn asymmetric_rsa_pss_and_ec_use_real_crypto() {
    let rsa_private = include_str!("jwt_fixtures/rsa-private.pem");
    let rsa_public = include_str!("jwt_fixtures/rsa-public.pem");
    for algorithm in [
        Alg::Rs256,
        Alg::Rs384,
        Alg::Rs512,
        Alg::Ps256,
        Alg::Ps384,
        Alg::Ps512,
    ] {
        let header = format!(r#"{{"alg":"{}"}}"#, algorithm.name());
        let token = jwt::sign(
            &header,
            r#"{"sub":"rsa-fixture"}"#,
            algorithm,
            rsa_private,
            Key::Text,
        )
        .unwrap();
        assert!(jwt::verify_signature(&token, algorithm, rsa_public, Key::Text).unwrap());
        let changed = token.replace("eyJzdWIiOiJyc2EtZml4dHVyZSJ9", "eyJzdWIiOiJ0YW1wZXJlZCJ9");
        assert_ne!(changed, token);
        assert!(!jwt::verify_signature(&changed, algorithm, rsa_public, Key::Text).unwrap());
    }
    let token = jwt::sign(
        r#"{"alg":"ES256"}"#,
        "{}",
        Alg::Es256,
        include_str!("jwt_fixtures/ec-private.pem"),
        Key::Text,
    )
    .unwrap();
    assert!(
        jwt::verify_signature(
            &token,
            Alg::Es256,
            include_str!("jwt_fixtures/ec-public.pem"),
            Key::Text
        )
        .unwrap()
    );
    assert!(
        jwt::sign(
            r#"{"alg":"RS256"}"#,
            "{}",
            Alg::Rs256,
            rsa_public,
            Key::Text
        )
        .is_err()
    );
}

#[test]
fn es384_and_eddsa_use_their_own_key_families() {
    for (algorithm, private, public) in [
        (
            Alg::Es384,
            include_str!("jwt_fixtures/ec384-private.pem"),
            include_str!("jwt_fixtures/ec384-public.pem"),
        ),
        (
            Alg::EdDsa,
            include_str!("jwt_fixtures/ed-private.pem"),
            include_str!("jwt_fixtures/ed-public.pem"),
        ),
    ] {
        let header = format!(r#"{{"alg":"{}"}}"#, algorithm.name());
        let token = jwt::sign(&header, "{}", algorithm, private, Key::Text).unwrap();
        assert!(jwt::verify_signature(&token, algorithm, public, Key::Text).unwrap());
    }
}

#[test]
fn signature_tampering_is_detected() {
    let mut token = PUBLIC_EXAMPLE.to_owned();
    let last = token.pop().unwrap();
    token.push(if last == 'A' { 'Q' } else { 'A' });
    assert!(!jwt::verify_signature(&token, Alg::Hs256, "your-256-bit-secret", Key::Text).unwrap());
}

#[test]
fn large_numbers_and_unicode_survive_roundtrip() {
    let payload = r#"{"编号":123456789012345678901234567890,"name":"测试 😀","decimal":1.234567890123456789}"#;
    let token = jwt::sign(r#"{"alg":"HS256"}"#, payload, Alg::Hs256, "test", Key::Text).unwrap();
    let decoded = jwt::decode(&token).unwrap();
    assert_eq!(
        decoded.payload["编号"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(
        decoded.payload["decimal"].to_string(),
        "1.234567890123456789"
    );
    assert_eq!(decoded.payload["name"], "测试 😀");
}

#[test]
fn bearer_prefix_is_accepted_but_internal_whitespace_is_not() {
    assert_eq!(
        jwt::decode(&format!("  Bearer {PUBLIC_EXAMPLE}\n"))
            .unwrap()
            .token,
        PUBLIC_EXAMPLE
    );
    assert!(jwt::decode(&PUBLIC_EXAMPLE.replace('.', ". ")).is_err());
}

#[test]
fn malformed_and_ambiguous_tokens_are_rejected() {
    for input in [
        "",
        "a.b",
        "a.b.c.d.e",
        "a..c",
        "e30=.e30.AA",
        "e30.e30.++",
        "_w.e30.AA",
    ] {
        assert!(
            jwt::decode(input).is_err(),
            "unexpectedly accepted: {input}"
        );
    }
    for header in [
        r#"{"alg":"none"}"#,
        r#"{"alg":"HS256","alg":"RS256"}"#,
        r#"{"alg":123}"#,
        r#"{"alg":"HS256","b64":false}"#,
        r#"{"alg":"HS256","crit":["unknown"]}"#,
        r#"{"alg":"hs256"}"#,
        "[]",
    ] {
        assert!(
            jwt::decode(&unsigned_fixture(header, "{}")).is_err(),
            "unexpectedly accepted: {header}"
        );
    }
    for payload in [
        "[]",
        r#"{"exp":1,"exp":2}"#,
        r#"{"nested":{"a":1,"\u0061":2}}"#,
    ] {
        assert!(jwt::decode(&unsigned_fixture(r#"{"alg":"HS256"}"#, payload)).is_err());
    }
    assert!(jwt::decode(&"A".repeat(jwt::MAX_TOKEN_BYTES + 1)).is_err());
}

#[test]
fn claim_times_distinguish_types_boundaries_and_fractional_seconds() {
    let claims = jwt::claim_times(&json!({"exp":100,"nbf":101,"iat":102}), 100);
    assert_eq!(claims[0].status, jwt::TimeStatus::Expired);
    assert_eq!(claims[1].status, jwt::TimeStatus::NotYetValid);
    assert_eq!(claims[2].status, jwt::TimeStatus::IssuedInFuture);
    let claims = jwt::claim_times(&json!({"exp":100.5,"nbf":"100","iat":-1}), 100);
    assert_eq!(claims[0].status, jwt::TimeStatus::Active);
    assert_eq!(claims[1].status, jwt::TimeStatus::Invalid);
    assert_eq!(claims[2].utc, "1969-12-31T23:59:59+00:00");
    assert_eq!(
        jwt::claim_times(&json!({"exp":i64::MAX}), 100)[0].status,
        jwt::TimeStatus::Invalid
    );
}
