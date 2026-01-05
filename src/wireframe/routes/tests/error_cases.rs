//! Unit tests covering routing error paths and state scaffolding.

#![expect(clippy::expect_used, reason = "test assertions")]

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use rstest::rstest;
use serial_test::serial;
use test_util::{AnyError, build_test_db, setup_files_db, setup_news_db};
use wireframe::middleware::{HandlerService, Service, ServiceRequest, ServiceResponse, Transform};

use super::{
    super::{
        TransactionMiddleware,
        dispatch_spy,
        error_reply,
        error_transaction,
        handle_command_parse_error,
        handle_parse_error,
        process_transaction_bytes,
    },
    helpers::{build_frame, runtime},
};
use crate::{
    field_id::FieldId,
    handler::Session,
    transaction::{FrameHeader, HEADER_LEN, Transaction, parse_transaction},
    transaction_type::TransactionType,
    wireframe::test_helpers::{dummy_pool, transaction_bytes},
};

/// Error code indicating a permission failure.
const ERR_PERMISSION: u32 = 1;
/// Error code for internal failures (mirrors the route module constant).
const ERR_INTERNAL: u32 = 3;
/// Error code returned for unknown transaction types (per spec: `ERR_INTERNAL`).
const ERR_UNKNOWN_TYPE: u32 = 3;

/// Parameterized test covering error reply scenarios.
///
/// Each case verifies that `error_reply` correctly constructs a reply with:
/// - `is_reply` set to 1
/// - The original transaction type preserved
/// - The original transaction ID preserved
/// - The specified error code applied
/// - An empty payload
#[rstest]
#[case::creates_valid_transaction(107, 12345, 1)]
#[case::preserves_transaction_id(200, 99999, ERR_INTERNAL)]
#[case::invalid_frame_returns_internal_error(0, 0, ERR_INTERNAL)]
#[case::unknown_type_returns_internal_error(65535, 1, ERR_INTERNAL)]
#[case::permission_error_preserves_type(200, 2, ERR_PERMISSION)]
#[case::preserves_id_for_unknown_type(65535, 12345, ERR_INTERNAL)]
fn error_reply_preserves_header_fields(#[case] ty: u16, #[case] id: u32, #[case] error_code: u32) {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty,
        id,
        error: 0,
        total_size: 0,
        data_size: 0,
    };

    let reply =
        error_reply(&header, error_code).expect("error_reply should succeed for valid header");

    assert_eq!(reply.header().is_reply, 1);
    assert_eq!(reply.header().ty, ty);
    assert_eq!(reply.header().id, id);
    assert_eq!(reply.header().error, error_code);
    assert!(reply.payload().is_empty());
}

/// Tests that malformed input returns an error with `ERR_INTERNAL`.
#[rstest]
fn handle_parse_error_returns_internal_error() {
    let result = handle_parse_error("simulated parse error");

    // Should produce a valid transaction header + empty payload.
    assert!(
        result.len() >= HEADER_LEN,
        "response too short to contain header"
    );

    let reply_header = FrameHeader::from_bytes(
        result[..HEADER_LEN]
            .try_into()
            .expect("header slice should be exact size"),
    );
    assert_eq!(reply_header.is_reply, 1);
    assert_eq!(reply_header.error, ERR_INTERNAL);
    assert_eq!(reply_header.data_size, 0);
}

/// Tests that command parse errors preserve the original header fields.
#[rstest]
fn handle_command_parse_error_preserves_id() {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 200,
        id: 54321,
        error: 0,
        total_size: 0,
        data_size: 0,
    };

    let result = handle_command_parse_error("simulated command error", &header);

    let reply_header = FrameHeader::from_bytes(
        result[..HEADER_LEN]
            .try_into()
            .expect("header slice should be exact size"),
    );
    assert_eq!(reply_header.is_reply, 1);
    assert_eq!(reply_header.ty, 200);
    assert_eq!(reply_header.id, 54321);
    assert_eq!(reply_header.error, ERR_INTERNAL);
}

/// Tests that `transaction_to_bytes` correctly serializes a transaction.
#[rstest]
fn transaction_to_bytes_roundtrip() {
    let tx = Transaction {
        header: FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: 107,
            id: 999,
            error: 0,
            total_size: 5,
            data_size: 5,
        },
        payload: b"hello".to_vec(),
    };

    let bytes = super::super::transaction_to_bytes(&tx);

    assert_eq!(bytes.len(), HEADER_LEN + 5);
    let parsed_header = FrameHeader::from_bytes(
        bytes[..HEADER_LEN]
            .try_into()
            .expect("header slice should be exact size"),
    );
    assert_eq!(parsed_header.id, 999);
    assert_eq!(parsed_header.ty, 107);
    assert_eq!(&bytes[HEADER_LEN..], b"hello");
}

/// Tests that `error_transaction` produces correct header values.
#[rstest]
fn error_transaction_sets_reply_flag() {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 370,
        id: 42,
        error: 0,
        total_size: 100,
        data_size: 100,
    };

    let err_tx = error_transaction(&header, ERR_INTERNAL);

    assert_eq!(err_tx.header.is_reply, 1);
    assert_eq!(err_tx.header.ty, 370);
    assert_eq!(err_tx.header.id, 42);
    assert_eq!(err_tx.header.error, ERR_INTERNAL);
    assert!(err_tx.payload.is_empty());
}

/// Tests that truncated input returns error.
#[rstest]
#[tokio::test]
async fn process_transaction_bytes_truncated_input() {
    let pool = dummy_pool();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse().expect("valid address");

    // Send only 10 bytes (less than HEADER_LEN = 20).
    let truncated = vec![0u8; 10];
    let result = process_transaction_bytes(&truncated, peer, pool, &mut session).await;

    // Should return an error transaction.
    assert!(result.len() >= HEADER_LEN);
    let reply_header = FrameHeader::from_bytes(
        result[..HEADER_LEN]
            .try_into()
            .expect("header slice should be exact size"),
    );
    assert_eq!(reply_header.error, ERR_INTERNAL);
}

/// Tests that unknown transaction type returns error code 3.
#[rstest]
#[tokio::test]
async fn process_transaction_bytes_unknown_type() {
    let pool = dummy_pool();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse().expect("valid address");

    // Create a transaction with unknown type (65535).
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 65535,
        id: 123,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let frame = transaction_bytes(&header, &[]);

    let result = process_transaction_bytes(&frame, peer, pool, &mut session).await;

    let reply_header = FrameHeader::from_bytes(
        result[..HEADER_LEN]
            .try_into()
            .expect("header slice should be exact size"),
    );
    assert_eq!(reply_header.is_reply, 1);
    assert_eq!(reply_header.id, 123, "transaction ID should be preserved");
    assert_eq!(reply_header.error, ERR_UNKNOWN_TYPE);
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn transaction_middleware_routes_known_types() -> Result<(), AnyError> {
    let rt = runtime();
    let Some(test_db) = build_test_db(&rt, setup_full_db)? else {
        return Ok(());
    };
    let pool = test_db.pool();
    let session = Arc::new(tokio::sync::Mutex::new(Session::default()));
    let peer = "127.0.0.1:12345".parse().expect("peer addr");
    let middleware = TransactionMiddleware::new(pool, Arc::clone(&session), peer);

    let calls = Arc::new(AtomicUsize::new(0));
    let spy = SpyService::new(Arc::clone(&calls));
    let service = HandlerService::from_service(0, spy);
    let wrapped = rt.block_on(middleware.transform(service));

    dispatch_spy::clear();
    let cases = build_middleware_cases();
    for case in &cases {
        let params: Vec<(FieldId, &[u8])> = case
            .params
            .iter()
            .map(|(field_id, data)| (*field_id, data.as_slice()))
            .collect();
        let frame = build_frame(case.ty, case.id, &params)?;
        let response = rt.block_on(wrapped.call(ServiceRequest::new(frame, None)))?;
        let reply = parse_transaction(response.frame())?;
        assert_eq!(reply.header.error, 0, "case {} failed", case.label);
    }

    let records = dispatch_spy::take();
    let expected_ids: HashSet<u32> = cases.iter().map(|case| case.id).collect();
    let mut records_by_id = HashMap::new();
    for record in records
        .into_iter()
        .filter(|record| expected_ids.contains(&record.id))
    {
        let record_id = record.id;
        let replaced = records_by_id.insert(record_id, record);
        assert!(
            replaced.is_none(),
            "duplicate dispatch record for id {record_id}"
        );
    }
    assert_eq!(records_by_id.len(), cases.len());
    for case in &cases {
        let record = records_by_id
            .get(&case.id)
            .expect("missing dispatch record");
        assert_eq!(record.peer, peer, "case {} peer mismatch", case.label);
        assert_eq!(
            record.ty,
            u16::from(case.ty),
            "case {} ty mismatch",
            case.label
        );
        assert_eq!(record.id, case.id, "case {} id mismatch", case.label);
    }
    assert_eq!(calls.load(Ordering::SeqCst), cases.len());
    Ok(())
}

struct MiddlewareCase {
    label: &'static str,
    ty: TransactionType,
    id: u32,
    params: Vec<(FieldId, Vec<u8>)>,
}

fn build_middleware_cases() -> Vec<MiddlewareCase> {
    let article_id = 1i32.to_be_bytes().to_vec();
    let flags = 0i32.to_be_bytes().to_vec();
    vec![
        MiddlewareCase {
            label: "login",
            ty: TransactionType::Login,
            id: 901_001,
            params: vec![
                (FieldId::Login, b"alice".to_vec()),
                (FieldId::Password, b"secret".to_vec()),
            ],
        },
        MiddlewareCase {
            label: "file_list",
            ty: TransactionType::GetFileNameList,
            id: 901_002,
            params: Vec::new(),
        },
        MiddlewareCase {
            label: "news_category_list",
            ty: TransactionType::NewsCategoryNameList,
            id: 901_003,
            params: Vec::new(),
        },
        MiddlewareCase {
            label: "news_article_list",
            ty: TransactionType::NewsArticleNameList,
            id: 901_004,
            params: vec![(FieldId::NewsPath, b"General".to_vec())],
        },
        MiddlewareCase {
            label: "news_article_data",
            ty: TransactionType::NewsArticleData,
            id: 901_005,
            params: vec![
                (FieldId::NewsPath, b"General".to_vec()),
                (FieldId::NewsArticleId, article_id),
            ],
        },
        MiddlewareCase {
            label: "post_news_article",
            ty: TransactionType::PostNewsArticle,
            id: 901_006,
            params: vec![
                (FieldId::NewsPath, b"General".to_vec()),
                (FieldId::NewsTitle, b"Third".to_vec()),
                (FieldId::NewsArticleFlags, flags),
                (FieldId::NewsDataFlavor, b"text/plain".to_vec()),
                (FieldId::NewsArticleData, b"hello".to_vec()),
            ],
        },
    ]
}

fn setup_full_db(db: &str) -> Result<(), AnyError> {
    setup_files_db(db)?;
    setup_news_db(db)?;
    Ok(())
}

struct SpyService {
    calls: Arc<AtomicUsize>,
}

impl SpyService {
    fn new(calls: Arc<AtomicUsize>) -> Self { Self { calls } }
}

#[async_trait]
impl Service for SpyService {
    type Error = std::convert::Infallible;

    async fn call(&self, req: ServiceRequest) -> Result<ServiceResponse, Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ServiceResponse::new(
            req.frame().to_vec(),
            req.correlation_id(),
        ))
    }
}
