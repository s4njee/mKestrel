//! Serve a file from `freya.local:/mnt/raid6/anime` (NFS) over the loopback
//! media server and print a VLC URL.
//!
//! Usage:
//!   cargo run -p mk-media --example serve_freya             # first video file
//!   cargo run -p mk-media --example serve_freya -- /mnt/raid6/anime/sub/file.mkv

use std::sync::Arc;

use mk_core::host::{AuthMethod, EntryKind, Host, HostOptions, HostStatus, Protocol};
use mk_media::{HostResolver, MediaServer};
use mk_vfs::{ConnectionPool, NfsBackend, VfsBackend};

const EXPORT: &str = "/mnt/raid6/anime";
const ADDR: &str = "freya.local";

#[tokio::main]
async fn main() {
    let path_arg = std::env::args().nth(1);

    let host = Host {
        id: "freya-anime".into(),
        name: "freya-anime".into(),
        group: "media".into(),
        protocol: Protocol::Nfs3,
        address: ADDR.into(),
        port: 2049,
        user: "nobody".into(),
        auth: AuthMethod::KbdInt,
        key_id: None,
        initial_path: EXPORT.into(),
        options: HostOptions::default(),
        status: HostStatus::Idle,
        free_bytes: None,
        rtt_ms: None,
        mounted_at: None,
        retrans: 0,
        is_real: true,
    };

    let host_for_factory = host.clone();
    let pool: Arc<ConnectionPool> = Arc::new(ConnectionPool::new(
        move |_: &Host| -> Arc<dyn VfsBackend> { Arc::new(NfsBackend::for_export(ADDR, EXPORT)) },
    ));
    let resolve: HostResolver = Arc::new(move |id| {
        if id == "freya-anime" {
            Some(host_for_factory.clone())
        } else {
            None
        }
    });

    let server = MediaServer::new(pool.clone(), resolve);
    let (addr, serve) = server.bind_local().await.expect("bind loopback");

    let path = match path_arg {
        Some(p) => p,
        None => {
            let backend = pool.get(&host).await;
            let entries = backend.list(EXPORT, None).await.expect("list export");
            let is_video = |name: &str| {
                let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                matches!(ext.as_str(), "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v")
            };
            let file = entries
                .iter()
                .find(|e| e.kind == EntryKind::File && is_video(&e.name))
                .or_else(|| entries.iter().find(|e| e.kind == EntryKind::File))
                .expect("no files in export");
            println!("listing {EXPORT}: {} entries", entries.len());
            format!("{EXPORT}/{}", file.name)
        }
    };

    let url = server.stream_url(addr.port(), "freya-anime", &path);
    println!("bound {}", addr);
    println!("open: {url}");
    println!("serving until Ctrl-C...");

    serve.await.expect("serve");
}
