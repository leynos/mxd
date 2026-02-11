//! Kani harnesses for client compatibility policy invariants.

use super::{
    ClientCompatibility,
    ClientKind,
    HOTLINE_19_MIN_VERSION,
    HOTLINE_85_MIN_VERSION,
    SYNHX_SUB_VERSION,
};
use crate::{protocol::VERSION, wireframe::connection::HandshakeMetadata};

fn handshake(sub_version: u16) -> HandshakeMetadata {
    HandshakeMetadata {
        sub_protocol: u32::from_be_bytes(*b"HOTL"),
        version: VERSION,
        sub_version,
    }
}

fn expected_kind(sub_version: u16, login_version: u16) -> ClientKind {
    if sub_version == SYNHX_SUB_VERSION {
        return ClientKind::SynHx;
    }
    if login_version >= HOTLINE_19_MIN_VERSION {
        return ClientKind::Hotline19;
    }
    if login_version >= HOTLINE_85_MIN_VERSION {
        return ClientKind::Hotline85;
    }
    ClientKind::Unknown
}

#[kani::proof]
fn kani_client_kind_sub_version_precedence() {
    let sub_version: u16 = kani::any();
    let login_version: u16 = kani::any();

    let compat = ClientCompatibility::from_handshake(&handshake(sub_version));
    compat.record_login_version(login_version);
    let expected_kind_value = expected_kind(sub_version, login_version);

    kani::assert(
        compat.kind() == expected_kind_value,
        "client kind classification matches version thresholds and SynHX precedence",
    );
}

#[kani::proof]
fn kani_login_extras_boundary_gate() {
    let sub_version: u16 = kani::any();
    let login_version: u16 = kani::any();

    let compat = ClientCompatibility::from_handshake(&handshake(sub_version));
    compat.record_login_version(login_version);

    let should_include = compat.should_include_login_extras();
    let expected = sub_version != SYNHX_SUB_VERSION && (login_version >= HOTLINE_85_MIN_VERSION);

    kani::assert(
        should_include == expected,
        "login extras are enabled only for Hotline 1.8.5+ when not SynHX",
    );
}
