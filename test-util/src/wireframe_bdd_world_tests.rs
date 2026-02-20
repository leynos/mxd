//! Tests for wireframe BDD world helper behaviour.

use std::time::Duration;

use super::WireframeBddWorld;

#[test]
fn set_io_timeout_overrides_default() {
    let world = WireframeBddWorld::new();
    world.set_io_timeout(Duration::from_secs(42));

    assert_eq!(world.get_io_timeout(), Duration::from_secs(42));
}
