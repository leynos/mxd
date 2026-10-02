//! Async bindings for every transaction-encoding feature scenario.

use rstest_bdd_macros::scenario;

use super::{EncodingWorld, world};

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Encodes a single-frame parameter transaction"
)]
#[tokio::test(flavor = "current_thread")]
async fn encodes_a_single_frame_parameter_transaction(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Encodes an empty-parameter transaction"
)]
#[tokio::test(flavor = "current_thread")]
async fn encodes_an_empty_parameter_transaction(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Encodes a fragmented parameter transaction"
)]
#[tokio::test(flavor = "current_thread")]
async fn encodes_a_fragmented_parameter_transaction(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Encodes a parameter transaction fragmented into 3 frames"
)]
#[tokio::test(flavor = "current_thread")]
async fn encodes_a_parameter_transaction_fragmented_into_3_frames(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Encodes a valid Transaction via TryFrom and matches legacy encoding"
)]
#[tokio::test(flavor = "current_thread")]
async fn encodes_a_valid_transaction_via_tryfrom_and_matches_legacy_encoding(
    _world: EncodingWorld,
) {
}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Rejects a Transaction with invalid flags"
)]
#[tokio::test(flavor = "current_thread")]
async fn rejects_a_transaction_with_invalid_flags(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Rejects a Transaction with an oversized payload"
)]
#[tokio::test(flavor = "current_thread")]
async fn rejects_a_transaction_with_an_oversized_payload(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Rejects a Transaction with an invalid parameter payload"
)]
#[tokio::test(flavor = "current_thread")]
async fn rejects_a_transaction_with_an_invalid_parameter_payload(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Rejects building a parameter transaction exceeding the limit"
)]
#[tokio::test(flavor = "current_thread")]
async fn rejects_building_a_parameter_transaction_exceeding_the_limit(_world: EncodingWorld) {}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    name = "Rejects encoding when the header size does not match the payload"
)]
#[tokio::test(flavor = "current_thread")]
async fn rejects_encoding_when_the_header_size_does_not_match_the_payload(_world: EncodingWorld) {}
