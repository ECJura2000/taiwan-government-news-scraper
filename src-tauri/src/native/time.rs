use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const SERVERS: [&str; 5] = [
    "time.stdtime.gov.tw",
    "tock.stdtime.gov.tw",
    "watch.stdtime.gov.tw",
    "clock.stdtime.gov.tw",
    "tick.stdtime.gov.tw",
];
const NTP_EPOCH: f64 = 2_208_988_800.0;

pub(super) struct RunClock {
    anchor: DateTime<Utc>,
    instant: Instant,
    pub(super) evidence: Value,
}
impl RunClock {
    pub(super) fn system() -> Self {
        Self {
            anchor: Utc::now(),
            instant: Instant::now(),
            evidence: json!({"status":"system_clock", "timezone":"Asia/Taipei", "protocol":"NTP", "port":123}),
        }
    }
    pub(super) fn now(&self) -> DateTime<Utc> {
        self.anchor + chrono::Duration::from_std(self.instant.elapsed()).unwrap_or_default()
    }
    pub(super) async fn synchronize() -> Self {
        let mut attempts = Vec::new();
        for server in SERVERS {
            let outcome = tokio::time::timeout(Duration::from_millis(1200), query(server)).await;
            match outcome {
                Ok(Ok((offset, delay, address))) => {
                    let mut clock = Self::system();
                    clock.anchor +=
                        chrono::Duration::microseconds((offset * 1_000_000.0).round() as i64);
                    attempts.push(json!({"server":server,"status":"success"}));
                    clock.evidence = json!({"status":"synchronized", "server":server, "address":address, "protocol":"NTP", "port":123,"timezone":"Asia/Taipei","offset_seconds":offset,"round_trip_seconds":delay,"synchronized_at":clock.anchor.to_rfc3339(),"attempts":attempts});
                    return clock;
                }
                Ok(Err(error)) => {
                    attempts.push(json!({"server":server,"status":"failed","error":error}))
                }
                Err(_) => {
                    attempts.push(json!({"server":server,"status":"failed","error":"timeout"}))
                }
            }
        }
        let mut clock = Self::system();
        clock.evidence["status"] = json!("unavailable_system_fallback");
        clock.evidence["attempts"] = json!(attempts);
        clock
    }
}
fn unix_seconds() -> f64 {
    Utc::now().timestamp_micros() as f64 / 1_000_000.0
}
fn encode_timestamp(time: f64) -> [u8; 8] {
    let ntp = time + NTP_EPOCH;
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&(ntp.floor() as u64 as u32).to_be_bytes());
    bytes[4..].copy_from_slice(&((ntp.fract() * 4_294_967_296.0) as u32).to_be_bytes());
    bytes
}
fn decode_timestamp(bytes: &[u8], reference: f64) -> f64 {
    let seconds = u32::from_be_bytes(bytes[..4].try_into().expect("timestamp seconds")) as f64;
    let fraction = u32::from_be_bytes(bytes[4..8].try_into().expect("timestamp fraction")) as f64
        / 4_294_967_296.0;
    let era = ((reference + NTP_EPOCH - seconds) / 4_294_967_296.0).round();
    seconds + era * 4_294_967_296.0 + fraction - NTP_EPOCH
}
fn sample(packet: &[u8], origin: &[u8; 8], t1: f64, t4: f64) -> Result<(f64, f64), String> {
    if packet.len() < 48
        || packet[0] >> 6 == 3
        || packet[0] & 7 != 4
        || !matches!((packet[0] >> 3) & 7, 3 | 4)
        || !(1..=15).contains(&packet[1])
    {
        return Err("invalid or unsynchronized NTP response".into());
    }
    if &packet[24..32] != origin
        || packet[32..40].iter().all(|b| *b == 0)
        || packet[40..48].iter().all(|b| *b == 0)
    {
        return Err("NTP origin or server timestamps do not match".into());
    }
    let dispersion =
        u32::from_be_bytes(packet[8..12].try_into().expect("dispersion")) as f64 / 65536.0;
    let t2 = decode_timestamp(&packet[32..40], t1);
    let t3 = decode_timestamp(&packet[40..48], t1);
    let delay = (t4 - t1) - (t3 - t2);
    let offset = ((t2 - t1) + (t3 - t4)) / 2.0;
    if dispersion > 5.0 || t3 < t2 || !(-0.001..=1.0).contains(&delay) || offset.abs() > 86400.0 {
        return Err("NTP sample exceeds precision or offset limits".into());
    }
    Ok((offset, delay.max(0.0)))
}
async fn query(server: &str) -> Result<(f64, f64, String), String> {
    let mut addresses: Vec<_> = tokio::net::lookup_host((server, 123))
        .await
        .map_err(|e| e.to_string())?
        .collect();
    addresses.sort_by_key(|a| a.is_ipv6());
    let mut errors: Vec<String> = Vec::new();
    for address in addresses {
        let attempt = async {
            let socket = UdpSocket::bind(if address.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .await
            .map_err(|e| e.to_string())?;
            socket.connect(address).await.map_err(|e| e.to_string())?;
            let t1 = unix_seconds();
            let instant = Instant::now();
            let origin = encode_timestamp(t1);
            let mut request = [0u8; 48];
            request[0] = 0x23;
            request[40..48].copy_from_slice(&origin);
            socket.send(&request).await.map_err(|e| e.to_string())?;
            let mut packet = [0u8; 512];
            let size = socket.recv(&mut packet).await.map_err(|e| e.to_string())?;
            let t4 = t1 + instant.elapsed().as_secs_f64();
            if (unix_seconds() - t4).abs() > 0.25 {
                return Err("system clock changed during query".into());
            }
            let (offset, delay) = sample(&packet[..size], &origin, t1, t4)?;
            Ok((offset, delay, address.to_string()))
        };
        match tokio::time::timeout(Duration::from_millis(450), attempt).await {
            Ok(Ok(result)) => return Ok(result),
            Ok(Err(error)) => errors.push(error),
            Err(_) => errors.push("address timeout".into()),
        }
    }
    Err(errors.join("; "))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_timestamps_validate_origin_and_calculate_offset() {
        let t1 = 1_791_763_200.0;
        let origin = encode_timestamp(t1);
        let mut packet = [0u8; 48];
        packet[0] = 0x24;
        packet[1] = 1;
        packet[24..32].copy_from_slice(&origin);
        packet[32..40].copy_from_slice(&encode_timestamp(t1 + 2.01));
        packet[40..48].copy_from_slice(&encode_timestamp(t1 + 2.02));
        let (offset, delay) = sample(&packet, &origin, t1, t1 + 0.03).unwrap();
        assert!((offset - 2.0).abs() < 0.00001);
        assert!((delay - 0.02).abs() < 0.00001);
        packet[24] ^= 1;
        assert!(sample(&packet, &origin, t1, t1 + 0.03).is_err());
        packet[24] ^= 1;
        packet[1] = 0;
        assert!(sample(&packet, &origin, t1, t1 + 0.03).is_err());
        packet[1] = 1;
        packet[0] = 0xe4;
        assert!(sample(&packet, &origin, t1, t1 + 0.03).is_err());
        assert!(sample(&packet[..20], &origin, t1, t1 + 0.03).is_err());
    }
    #[test]
    fn timestamps_round_trip_across_2036_era_boundary() {
        for timestamp in [1_791_763_200.0, 2_200_000_000.25] {
            assert!(
                (decode_timestamp(&encode_timestamp(timestamp), timestamp) - timestamp).abs()
                    < 0.00001
            );
        }
    }
}
