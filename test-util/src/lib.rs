use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

#[cfg(feature = "postgres")]
use postgresql_embedded::PostgreSQL;

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;

/// Helper struct to start the mxd server for integration tests.
///
/// The server process is terminated when this struct is dropped.
pub struct TestServer {
    child: Child,
    port: u16,
    db_url: String,
    _temp: TempDir,
    #[cfg(feature = "postgres")]
    pg: Option<PostgreSQL>,
}

#[cfg(feature = "sqlite")]
fn setup_sqlite<F>(temp: &TempDir, setup: F) -> Result<String, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
{
    let path = temp.path().join("mxd.db");
    setup(path.to_str().expect("db path utf8"))?;
    Ok(path.to_str().unwrap().to_owned())
}

#[cfg(feature = "postgres")]
fn setup_postgres<F>(setup: F) -> Result<(String, PostgreSQL), Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
{
    let mut pg = PostgreSQL::default();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        pg.setup().await?;
        pg.start().await?;
        pg.create_database("test").await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    let url = pg.settings().url("test");
    setup(&url)?;
    Ok((url, pg))
}

fn wait_for_server(child: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(out) = &mut child.stdout {
        let mut reader = BufReader::new(out);
        let mut line = String::new();
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                return Err("server exited before signaling readiness".into());
            }
            if line.contains("listening on") {
                break;
            }
            if start.elapsed() > timeout {
                return Err("timeout waiting for server to signal readiness".into());
            }
        }
        Ok(())
    } else {
        Err("missing stdout from server".into())
    }
}

fn build_server_command(manifest_path: &str, port: u16, db_url: &str) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--bin",
        "mxd",
        "--manifest-path",
        manifest_path,
        "--quiet",
        "--",
        "--bind",
        &format!("127.0.0.1:{port}"),
        "--database",
        db_url,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit());
    cmd
}

impl TestServer {
    /// Start the server using the given Cargo manifest path.
    pub fn start(manifest_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    /// Start the server and run a setup function before launching.
    pub fn start_with_setup<F>(
        manifest_path: &str,
        setup: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
    {
        let temp = TempDir::new()?;

        #[cfg(feature = "sqlite")]
        let db_url = setup_sqlite(&temp, setup)?;

        #[cfg(feature = "postgres")]
        let (db_url, mut pg) = setup_postgres(setup)?;

        #[cfg(feature = "postgres")]
        let db_url = db_url;

        let socket = TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);

        let mut child = build_server_command(manifest_path, port, &db_url).spawn()?;

        wait_for_server(&mut child)?;

        Ok(Self {
            child,
            port,
            db_url,
            _temp: temp,
            #[cfg(feature = "postgres")]
            pg: Some(pg),
        })
    }

    /// Return the port the server is bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Return the database connection URL used by the server.
    ///
    /// The URL is stored as a `String` validated at construction time.
    /// Tests use SQLite by default, with optional PostgreSQL support via the
    /// `postgres` feature.
    pub fn db_url(&self) -> &str {
        // `db_url` was validated when the server was created, so borrowing is
        // safe and avoids repeated validation.
        self.db_url.as_str()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        #[cfg(feature = "postgres")]
        if let Some(pg) = &mut self.pg {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(pg.stop());
        }
    }
}

use std::io::{Read, Write};
use std::net::TcpStream;

/// Send a valid protocol handshake and read the server reply.
pub fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"TRTP");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&buf)?;
    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(
        &reply[0..4],
        b"TRTP",
        "protocol mismatch in handshake reply"
    );
    let code = u32::from_be_bytes(reply[4..8].try_into().unwrap());
    assert_eq!(code, 0, "handshake returned error code {}", code);
    Ok(())
}

use chrono::{DateTime, Utc};

use diesel_async::{AsyncConnection, RunQueryDsl};
use futures_util::future::BoxFuture;
use mxd::db::{
    DbConnection, add_file_acl, apply_migrations, create_category, create_file, create_user,
};
use mxd::models::{NewArticle, NewCategory, NewFileAcl, NewFileEntry, NewUser};
use mxd::users::hash_password;

/// Executes an asynchronous database setup function within a temporary Tokio runtime.
///
/// Establishes a database connection, runs migrations, and invokes the provided async closure with the connection. Suitable for preparing test databases synchronously from non-async contexts.
///
/// # Parameters
/// - `db`: Database connection string.
///
/// # Returns
/// Returns `Ok(())` if the setup function completes successfully; otherwise, returns an error.
pub fn with_db<F>(db: &str, f: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: for<'c> FnOnce(
        &'c mut DbConnection,
    ) -> BoxFuture<'c, Result<(), Box<dyn std::error::Error>>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db).await?;
        apply_migrations(&mut conn, db).await?;
        f(&mut conn).await
    })
}

/// Populate the database with sample files and ACLs for file-related tests.
pub fn setup_files_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            let argon2 = argon2::Argon2::default();
            let hashed = hash_password(&argon2, "secret")?;
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(conn, &new_user).await?;
            let files = [
                NewFileEntry {
                    name: "fileA.txt",
                    object_key: "1",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileB.txt",
                    object_key: "2",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileC.txt",
                    object_key: "3",
                    size: 1,
                },
            ];
            for file in &files {
                create_file(conn, file).await?;
            }
            let acls = [
                NewFileAcl {
                    file_id: 1,
                    user_id: 1,
                },
                NewFileAcl {
                    file_id: 3,
                    user_id: 1,
                },
            ];
            for acl in &acls {
                add_file_acl(conn, acl).await?;
            }
            Ok(())
        })
    })
}

/// Populate the database with a "General" category and a couple of articles.
pub fn setup_news_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            use mxd::schema::news_articles::dsl as a;
            let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: None,
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("a"),
                })
                .execute(conn)
                .await?;
            let posted2 = DateTime::<Utc>::from_timestamp(2000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: Some(1),
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "Second",
                    poster: None,
                    posted_at: posted2,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("b"),
                })
                .execute(conn)
                .await?;
            Ok(())
        })
    })
}

/// Populate the database with a bundle and two categories at the root level.
pub fn setup_news_categories_root_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            use mxd::db::create_category;

            let _ = insert_root_bundle(conn).await?;

            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "Updates",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })
}

/// Populate the database with a nested bundle containing a single category.
pub fn setup_news_categories_nested_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            use mxd::db::{create_bundle, create_category};
            use mxd::models::NewBundle;

            let root_id = insert_root_bundle(conn).await?;

            let sub_id = create_bundle(
                conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;

            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "Inside",
                    bundle_id: Some(sub_id),
                },
            )
            .await?;
            Ok(())
        })
    })
}

async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, Box<dyn std::error::Error>> {
    use mxd::db::create_bundle;
    use mxd::models::NewBundle;

    let id = create_bundle(
        conn,
        &NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        },
    )
    .await?;

    Ok(id)
}
