//! Tests for command parsing helpers.

use rstest::rstest;

use super::*;
use crate::transaction::encode_params;

/// Returns valid login parameters for testing.
fn valid_login_payload() -> Vec<u8> {
    let params: Vec<(FieldId, &[u8])> =
        vec![(FieldId::Login, b"alice"), (FieldId::Password, b"secret")];
    encode_params(&params).expect("payload encodes")
}

/// Asserts that credentials match expected valid values.
fn assert_valid_credentials(creds: &LoginCredentials) {
    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "secret");
}

#[test]
fn parse_login_params_both_fields_valid() {
    let payload = valid_login_payload();
    let result = parse_login_params(&payload).expect("should parse");
    assert_valid_credentials(&result);
}

#[test]
fn parse_login_params_missing_username() {
    let params: Vec<(FieldId, &[u8])> = vec![(FieldId::Password, b"secret")];
    let payload = encode_params(&params).expect("payload encodes");
    let result = parse_login_params(&payload);
    assert!(matches!(
        result,
        Err(TransactionError::MissingField(FieldId::Login))
    ));
}

#[test]
fn parse_login_params_missing_password() {
    let params: Vec<(FieldId, &[u8])> = vec![(FieldId::Login, b"alice")];
    let payload = encode_params(&params).expect("payload encodes");
    let result = parse_login_params(&payload);
    assert!(matches!(
        result,
        Err(TransactionError::MissingField(FieldId::Password))
    ));
}

#[rstest]
#[case(FieldId::Login, FieldId::Login)]
#[case(FieldId::Password, FieldId::Password)]
fn parse_login_params_invalid_utf8(
    #[case] invalid_field: FieldId,
    #[case] expected_error_field: FieldId,
) {
    let params: Vec<(FieldId, Vec<u8>)> = vec![
        (
            FieldId::Login,
            if invalid_field == FieldId::Login {
                vec![0xff, 0xfe]
            } else {
                b"alice".to_vec()
            },
        ),
        (
            FieldId::Password,
            if invalid_field == FieldId::Password {
                vec![0xff, 0xfe]
            } else {
                b"secret".to_vec()
            },
        ),
    ];
    let payload = encode_params(&params).expect("payload encodes");
    let result = parse_login_params(&payload);
    assert!(matches!(
        result,
        Err(TransactionError::InvalidParamValue(field)) if field == expected_error_field
    ));
}

#[test]
fn parse_login_params_ignores_extra_fields() {
    let mut params: Vec<(FieldId, Vec<u8>)> = vec![
        (FieldId::Login, b"alice".to_vec()),
        (FieldId::Password, b"secret".to_vec()),
    ];
    params.push((FieldId::NewsPath, b"/news".to_vec()));
    let payload = encode_params(&params).expect("payload encodes");
    let result = parse_login_params(&payload).expect("should parse");
    assert_valid_credentials(&result);
}

#[test]
fn parse_login_params_rejects_malformed_payload() {
    // Payload too short to contain the parameter count (needs at least 2 bytes)
    let malformed = &[0x01];
    let result = parse_login_params(malformed);
    assert!(matches!(result, Err(TransactionError::SizeMismatch)));
}
