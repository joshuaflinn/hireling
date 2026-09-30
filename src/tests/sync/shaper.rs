//! Dev-only TCP shaper (plan Task 14): a loopback proxy that delays every
//! chunk by a freshly sampled one-way latency — `one_way ± jitter`, with a
//! retransmit-class spike on `loss_percent` of transfers — under a seeded
//! xorshift RNG, so the FR-5 budgets are measured reproducibly against the
//! real server. Nothing here runs outside `cfg(test)`.

/// One shaped link profile. The FR-5 numbers themselves live in the tests
/// that assert them; this type only carries the wire shape.
#[derive(Clone, Copy)]
pub struct Profile {
    /// One-way latency, milliseconds.
    pub one_way_ms: u64,
    /// ± jitter around the one-way latency, milliseconds.
    pub jitter_ms: u64,
    /// Percent of transfers hit by a retransmit-class delay spike.
    pub loss_percent: u64,
}

/// `{50 ms, ±20, 0%}` (plan Task 14).
pub const WIFI: Profile = Profile {
    one_way_ms: 50,
    jitter_ms: 20,
    loss_percent: 0,
};

/// `{300 ms, ±100, 1%}` (plan Task 14).
pub const CELLULAR: Profile = Profile {
    one_way_ms: 300,
    jitter_ms: 100,
    loss_percent: 1,
};

/// xorshift64 — deterministic, dependency-free.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// The one-way delay for one transfer. A userspace TCP proxy cannot
    /// drop in-stream bytes, so "loss" is modeled as the delay a dropped
    /// segment's retransmit would add (one extra one-way + jitter on top).
    fn delay_ms(&mut self, profile: &Profile) -> u64 {
        let span = (2 * profile.jitter_ms).max(1);
        let jitter = i64::try_from(self.next_u64() % span).unwrap_or(0);
        let signed = jitter - i64::try_from(profile.jitter_ms).unwrap_or(0);
        let one_way = i64::try_from(profile.one_way_ms).unwrap_or(0);
        let mut delay = (one_way + signed).max(0);
        if profile.loss_percent > 0 && self.next_u64() % 100 < profile.loss_percent {
            delay += one_way + i64::try_from(profile.jitter_ms).unwrap_or(0);
        }
        u64::try_from(delay).unwrap_or(0)
    }
}

/// Spawn the proxy; returns the local address clients should point at.
/// Each accepted connection is pumped in both directions and dies with its
/// sockets — the proxy holds no state across them.
pub async fn spawn(
    target: std::net::SocketAddr,
    profile: Profile,
    seed: u64,
) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("shaper bind");
    let addr = listener.local_addr().expect("shaper addr");
    tokio::spawn(async move {
        loop {
            let (client, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(err) => {
                    eprintln!("shaper accept error: {err}");
                    break;
                }
            };
            let downstream = match tokio::net::TcpStream::connect(target).await {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("shaper connect to {target} failed: {err}");
                    continue;
                }
            };
            if client.set_nodelay(true).is_err() || downstream.set_nodelay(true).is_err() {
                eprintln!("shaper: set_nodelay failed (latency shape still holds)");
            }
            let (client_read, client_write) = client.into_split();
            let (down_read, down_write) = downstream.into_split();
            // Direction tags split one seed into two deterministic streams.
            tokio::spawn(pump(
                client_read,
                down_write,
                profile,
                seed ^ 0x9E37_79B9_7F4A_7C15,
            ));
            tokio::spawn(pump(
                down_read,
                client_write,
                profile,
                seed ^ 0x632B_E59B_D9B4_E019,
            ));
        }
    });
    addr
}

/// Pump one direction: every read chunk crosses after a freshly sampled
/// one-way delay. The chunk (not the frame) is the sampling unit — the
/// kernel decides chunking, and a small frame is one chunk in practice.
async fn pump(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    profile: Profile,
    seed: u64,
) {
    let mut rng = Rng(seed | 1);
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        match tokio::io::AsyncReadExt::read(&mut reader, &mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tokio::time::sleep(std::time::Duration::from_millis(rng.delay_ms(&profile))).await;
                let bytes = chunk.get(..n).expect("read() bounds n by the buffer");
                if tokio::io::AsyncWriteExt::write_all(&mut writer, bytes)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    if tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .is_err()
    {
        eprintln!("shaper: shutdown failed on an already-gone socket");
    }
}
