//! 네트워크 환경 인식.
//!
//! Wi-Fi/네트워크는 "인터넷을 쓰기 위한 것"이 아니라 기기가 지금 어떤 네트워크에
//! 노출되어 있는지 파악하는 용도다. 결과는 엔진 선택(온라인/오프라인)과
//! 시스템 프롬프트의 "현재 상태"에 쓰인다.
//!
//! - Linux: `/sys/class/net`, `ip -j addr`, `nmcli`, `mmcli` 로 직접 수집
//! - 그 밖의 OS: C# 컴패니언(`alliedid-companion net --json`)에 위임
//! - 둘 다 안 되면 TCP 접속 시험만 수행(probe-only)

use crate::config::NetConfig;
use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Iface {
    pub name: String,
    /// wifi | ethernet | cellular | tethering | vpn | virtual | other
    pub kind: String,
    pub up: bool,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WifiInfo {
    pub ssid: String,
    pub security: String,
    pub signal: Option<u8>,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetReport {
    pub source: String,
    pub online: bool,
    pub reachable: Vec<String>,
    pub interfaces: Vec<Iface>,
    pub wifi: Option<WifiInfo>,
    pub cellular: Vec<String>,
    pub warnings: Vec<String>,
}

impl NetReport {
    fn active_kinds(&self) -> Vec<&str> {
        let mut kinds: Vec<&str> = self
            .interfaces
            .iter()
            .filter(|i| i.up && i.kind != "virtual")
            .map(|i| i.kind.as_str())
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// 사람이 읽을 한 줄 요약. 시스템 프롬프트에도 들어간다.
    pub fn summary(&self) -> String {
        let state = if self.online { "온라인" } else { "오프라인" };
        let mut parts = Vec::new();
        for kind in self.active_kinds() {
            match kind {
                "wifi" => match &self.wifi {
                    Some(w) => parts.push(format!("Wi-Fi \"{}\" ({})", w.ssid, if w.open { "암호 없음" } else { &w.security })),
                    None => parts.push("Wi-Fi".to_string()),
                },
                "ethernet" => parts.push("유선".to_string()),
                "cellular" => parts.push("셀룰러".to_string()),
                "tethering" => parts.push("테더링".to_string()),
                "vpn" => parts.push("VPN".to_string()),
                other => parts.push(other.to_string()),
            }
        }
        if parts.is_empty() {
            state.to_string()
        } else {
            format!("{state} · {}", parts.join(", "))
        }
    }
}

pub fn scan(cfg: &NetConfig) -> NetReport {
    let mut report = platform_scan()
        .or_else(companion_scan)
        .unwrap_or_else(|| NetReport { source: "probe-only".into(), ..Default::default() });
    report.reachable = probe(&cfg.probe_targets, Duration::from_millis(cfg.probe_timeout_ms));
    report.online = !report.reachable.is_empty();
    report.warnings = analyze(&report);
    report
}

/// 대상마다 별도 스레드에서 DNS 조회 + TCP 접속을 시도한다(DNS가 멈춰도 timeout 안에 끝남).
pub fn probe(targets: &[String], timeout: Duration) -> Vec<String> {
    let (tx, rx) = mpsc::channel();
    for target in targets {
        let tx = tx.clone();
        let target = target.clone();
        thread::spawn(move || {
            let ok = target
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
                .is_some_and(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok());
            let _ = tx.send((target, ok));
        });
    }
    drop(tx);

    let deadline = std::time::Instant::now() + timeout + Duration::from_millis(500);
    let mut reachable = Vec::new();
    for _ in targets {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok((t, true)) => reachable.push(t),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    reachable.sort();
    reachable
}

pub fn analyze(r: &NetReport) -> Vec<String> {
    let mut w = Vec::new();
    let kinds = r.active_kinds();
    if let Some(wifi) = &r.wifi {
        if wifi.open {
            w.push(format!(
                "암호가 없는 개방형 Wi-Fi(\"{}\")에 연결되어 있습니다. 같은 네트워크의 다른 기기가 트래픽을 볼 수 있습니다.",
                wifi.ssid
            ));
        } else if wifi.security.contains("WEP") || wifi.security.contains("TKIP") {
            w.push(format!("Wi-Fi 보안 방식({})이 오래되어 취약합니다.", wifi.security));
        }
        if let Some(s) = wifi.signal {
            if s < 30 {
                w.push(format!("Wi-Fi 신호가 약합니다({s}%)."));
            }
        }
    }
    if kinds.contains(&"cellular") || kinds.contains(&"tethering") {
        w.push("셀룰러/테더링 경로가 활성화되어 있습니다. 사용자의 요금제 데이터가 소모될 수 있습니다.".into());
    }
    if !r.online && kinds.iter().any(|k| *k != "vpn") {
        w.push("네트워크 연결은 있지만 인터넷에 닿지 않습니다(캡티브 포털이나 차단된 네트워크일 수 있음).".into());
    }
    w
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `nmcli -t` 출력 한 줄을 나눈다. 값 안의 `:` 는 `\:` 로 이스케이프되어 있다.
pub fn split_nmcli(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    fields.last_mut().unwrap().push(n);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

pub fn parse_nmcli_wifi(output: &str) -> Option<WifiInfo> {
    output.lines().find_map(|line| {
        let f = split_nmcli(line);
        if f.len() < 4 || f[0] != "yes" {
            return None;
        }
        let security = f[2].trim().to_string();
        let open = security.is_empty() || security == "--";
        Some(WifiInfo {
            ssid: f[1].clone(),
            security: if open { "없음".into() } else { security },
            signal: f[3].trim().parse().ok(),
            open,
        })
    })
}

#[cfg(target_os = "linux")]
fn platform_scan() -> Option<NetReport> {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    fn read(p: &Path) -> String {
        fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    fn classify(name: &str, base: &Path) -> &'static str {
        let driver = fs::read_link(base.join("device/driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        if base.join("wireless").exists() || base.join("phy80211").exists() {
            "wifi"
        } else if name.starts_with("wwan")
            || name.starts_with("ppp")
            || matches!(driver.as_str(), "qmi_wwan" | "cdc_mbim" | "option" | "huawei_cdc_ncm")
        {
            "cellular"
        } else if name.starts_with("usb") || matches!(driver.as_str(), "rndis_host" | "ipheth") {
            "tethering"
        } else if ["tun", "tap", "wg", "tailscale", "zt", "utun"].iter().any(|p| name.starts_with(p)) {
            "vpn"
        } else if ["docker", "br-", "virbr", "veth", "vmnet", "lxc"].iter().any(|p| name.starts_with(p))
            || !base.join("device").exists()
        {
            "virtual"
        } else if read(&base.join("type")) == "1" {
            "ethernet"
        } else {
            "other"
        }
    }

    // `ip -j addr` 는 iproute2 가 있으면 인터페이스별 주소를 JSON 으로 준다.
    let mut addrs: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(json) = run("ip", &["-j", "addr"]) {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&json) {
            for item in items {
                let name = item["ifname"].as_str().unwrap_or_default().to_string();
                let list = item["addr_info"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| Some(format!("{}/{}", x["local"].as_str()?, x["prefixlen"].as_u64()?)))
                            .collect()
                    })
                    .unwrap_or_default();
                addrs.insert(name, list);
            }
        }
    }

    let mut interfaces = Vec::new();
    for entry in fs::read_dir("/sys/class/net").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "lo" {
            continue;
        }
        let base = entry.path();
        let oper = read(&base.join("operstate"));
        let up = oper == "up" || (oper == "unknown" && read(&base.join("carrier")) == "1");
        interfaces.push(Iface {
            kind: classify(&name, &base).to_string(),
            up,
            addresses: addrs.remove(&name).unwrap_or_default(),
            name,
        });
    }
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    let wifi = run("nmcli", &["-t", "-f", "ACTIVE,SSID,SECURITY,SIGNAL", "dev", "wifi", "list", "--rescan", "no"])
        .and_then(|o| parse_nmcli_wifi(&o));

    let cellular = run("mmcli", &["-L"])
        .map(|o| {
            o.lines()
                .map(str::trim)
                .filter(|l| l.contains("/Modem/"))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    Some(NetReport {
        source: "linux-sysfs".into(),
        interfaces,
        wifi,
        cellular,
        ..Default::default()
    })
}

#[cfg(not(target_os = "linux"))]
fn platform_scan() -> Option<NetReport> {
    None
}

/// 같은 디렉터리, PATH, 데이터 디렉터리 순으로 C# 컴패니언을 찾는다.
pub fn companion_path() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "alliedid-companion.exe" } else { "alliedid-companion" };
    let mut candidates = Vec::new();
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            candidates.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(exe)));
    }
    candidates.push(crate::paths::data_dir().join("companion").join(exe));
    candidates.into_iter().find(|p| p.is_file())
}

fn companion_scan() -> Option<NetReport> {
    let exe = companion_path()?;
    let out = Command::new(exe).args(["net", "--json", "--no-probe"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let mut r: NetReport = serde_json::from_slice(&out.stdout).ok()?;
    r.source = "companion".into();
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nmcli_escaped_colon() {
        assert_eq!(split_nmcli(r"yes:Cafe\:Free::72"), vec!["yes", "Cafe:Free", "", "72"]);
    }

    #[test]
    fn nmcli_active_open_network() {
        let out = "no:Home:WPA2:80\nyes:Cafe\\:Free::72\n";
        let w = parse_nmcli_wifi(out).unwrap();
        assert_eq!(w.ssid, "Cafe:Free");
        assert!(w.open);
        assert_eq!(w.signal, Some(72));
    }

    #[test]
    fn warnings_for_open_wifi_and_cellular() {
        let r = NetReport {
            online: true,
            interfaces: vec![
                Iface { name: "wlan0".into(), kind: "wifi".into(), up: true, addresses: vec![] },
                Iface { name: "wwan0".into(), kind: "cellular".into(), up: true, addresses: vec![] },
            ],
            wifi: Some(WifiInfo { ssid: "Free".into(), security: "없음".into(), signal: Some(90), open: true }),
            ..Default::default()
        };
        let w = analyze(&r);
        assert!(w.iter().any(|m| m.contains("개방형")));
        assert!(w.iter().any(|m| m.contains("셀룰러")));
        assert!(r.summary().starts_with("온라인"));
    }

    #[test]
    fn captive_portal_warning_when_link_but_offline() {
        let r = NetReport {
            online: false,
            interfaces: vec![Iface { name: "eth0".into(), kind: "ethernet".into(), up: true, addresses: vec![] }],
            ..Default::default()
        };
        assert!(analyze(&r).iter().any(|m| m.contains("인터넷에 닿지")));
    }
}
