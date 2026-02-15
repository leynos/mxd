//! Integration tests for `TransactionMiddleware` routing.

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use rstest::rstest;
use serial_test::serial;
use test_util::{AnyError, DatabaseUrl, build_test_db, setup_files_db, setup_news_db};
use tokio::runtime::Runtime;
use wireframe::middleware::{HandlerService, Service, ServiceRequest, ServiceResponse, Transform};

use super::{
    super::{TransactionMiddleware, TransactionMiddlewareConfig, dispatch_spy},
    helpers::{build_frame, runtime},
};
use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    server::outbound::NoopOutboundMessaging,
    transaction::parse_transaction,
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        router::WireframeRouter,
    },
};

/// Build the wrapped middleware service, peer address, and call counter.
fn setup_middleware_test(
    rt: &Runtime,
    pool: DbPool,
) -> (
    impl Service<Error: Into<AnyError>>,
    SocketAddr,
    Arc<AtomicUsize>,
) {
    let session = Arc::new(tokio::sync::Mutex::new(Session::default()));
    let peer: SocketAddr = "127.0.0.1:12345"
        .parse()
        .unwrap_or_else(|err| panic!("failed to parse fixture peer address: {err}"));
    let messaging = Arc::new(NoopOutboundMessaging);
    let router = WireframeRouter::new(
        Arc::new(XorCompatibility::disabled()),
        Arc::new(ClientCompatibility::from_handshake(
            &HandshakeMetadata::default(),
        )),
    );
    let middleware = TransactionMiddleware::new(TransactionMiddlewareConfig {
        router,
        pool,
        session,
        peer,
        messaging,
    });

    let calls = Arc::new(AtomicUsize::new(0));
    let spy = SpyService::new(Arc::clone(&calls));
    let service = HandlerService::from_service(0, spy);
    let wrapped = rt.block_on(middleware.transform(service));
    (wrapped, peer, calls)
}

/// Route each test case through the middleware and assert success.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
fn execute_test_cases(
    rt: &Runtime,
    wrapped: &impl Service<Error: Into<AnyError>>,
) -> Result<(), AnyError> {
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
    Ok(())
}

/// Verify dispatch spy records match the expected test cases.
/// Verify dispatch spy records match the expected test cases.
fn verify_dispatch_records(peer: SocketAddr, calls: &Arc<AtomicUsize>) {
    let cases = build_middleware_cases();
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
            .unwrap_or_else(|| panic!("missing dispatch record for case {}", case.label));
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
}

#[rstest]
#[serial]
fn transaction_middleware_routes_known_types() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_full_db)? else {
        return Ok(());
    };

    let (wrapped, peer, calls) = setup_middleware_test(&rt, test_db.pool());
    execute_test_cases(&rt, &wrapped)?;
    verify_dispatch_records(peer, &calls);
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

fn setup_full_db(db: DatabaseUrl) -> Result<(), AnyError> {
    setup_files_db(db.clone())?;
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
