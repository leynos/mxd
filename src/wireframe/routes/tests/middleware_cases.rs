//! Integration tests for `TransactionMiddleware` routing.

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
use test_util::{AnyError, DatabaseUrl, build_test_db, setup_files_db, setup_news_db};
use wireframe::middleware::{HandlerService, Service, ServiceRequest, ServiceResponse, Transform};

use super::{
    super::{TransactionMiddleware, TransactionMiddlewareConfig, dispatch_spy},
    helpers::{build_frame, runtime},
};
use crate::{
    field_id::FieldId,
    handler::Session,
    server::outbound::NoopOutboundMessaging,
    transaction::parse_transaction,
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
    },
};

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn transaction_middleware_routes_known_types() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_full_db)? else {
        return Ok(());
    };
    let pool = test_db.pool();
    // Start with an unauthenticated session; login should establish privileges.
    let session = Arc::new(tokio::sync::Mutex::new(Session::default()));
    let peer = "127.0.0.1:12345".parse().expect("peer addr");
    let messaging = Arc::new(NoopOutboundMessaging);
    let compat = Arc::new(XorCompatibility::disabled());
    let client_compat = Arc::new(ClientCompatibility::from_handshake(
        &HandshakeMetadata::default(),
    ));
    let middleware = TransactionMiddleware::new(TransactionMiddlewareConfig {
        pool,
        session: Arc::clone(&session),
        peer,
        messaging,
        compat,
        client_compat,
    });

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
