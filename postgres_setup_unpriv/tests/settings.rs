use std::path::PathBuf;

use postgres_setup_unpriv::{with_temp_euid, PgEnvCfg};
use postgresql_embedded::VersionReq;
use nix::unistd::{geteuid, Uid};
use rstest::rstest;

#[rstest]
fn to_settings_roundtrip() -> anyhow::Result<()> {
    let cfg = PgEnvCfg {
        version_req: Some("=16.4.0".into()),
        port: Some(5433),
        superuser: Some("admin".into()),
        password: Some("secret".into()),
        data_dir: Some(PathBuf::from("/tmp/data")),
        runtime_dir: Some(PathBuf::from("/tmp/runtime")),
        cache_dir: Some(PathBuf::from("/tmp/cache")),
        locale: Some("en_US".into()),
        encoding: Some("UTF8".into()),
        auth_method: Some("trust".into()),
    };
    let settings = cfg.to_settings()?;
    assert_eq!(settings.version, VersionReq::parse("=16.4.0")?);
    assert_eq!(settings.port, 5433);
    assert_eq!(settings.username, "admin");
    assert_eq!(settings.password, "secret");
    assert_eq!(settings.data_dir, PathBuf::from("/tmp/data"));
    // cache_dir overwrites runtime_dir in current implementation
    assert_eq!(settings.installation_dir, PathBuf::from("/tmp/cache"));
    assert_eq!(settings.configuration.get("locale"), Some(&"en_US".to_string()));
    assert_eq!(settings.configuration.get("encoding"), Some(&"UTF8".to_string()));
    assert_eq!(settings.configuration.get("auth_method"), Some(&"trust".to_string()));
    Ok(())
}

#[rstest]
fn to_settings_invalid_auth() {
    let cfg = PgEnvCfg {
        auth_method: Some("invalid".into()),
        ..Default::default()
    };
    assert!(cfg.to_settings().is_err());
}

#[rstest]
fn with_temp_euid_changes_uid() -> anyhow::Result<()> {
    let original = geteuid();
    assert!(original.is_root());

    with_temp_euid(Uid::from_raw(65534), || {
        assert_eq!(geteuid(), Uid::from_raw(65534));
        Ok(())
    })?;

    assert_eq!(geteuid(), original);
    Ok(())
}
