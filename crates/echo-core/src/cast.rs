use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastProtocol {
    Chromecast,
    Dlna,
    FCast,
}

impl CastProtocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chromecast => "Google Cast / Chromecast",
            Self::Dlna => "DLNA / UPnP Renderer",
            Self::FCast => "FCast Open Wireless",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Chromecast => 8009,
            Self::Dlna => 1400,
            Self::FCast => 46899,
        }
    }
}

impl std::fmt::Display for CastProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastDevice {
    pub id: String,
    pub name: String,
    pub protocol: CastProtocol,
    pub address: String,
    pub model: Option<String>,
}

impl CastDevice {
    pub fn new(name: impl Into<String>, protocol: CastProtocol, address: impl Into<String>) -> Self {
        let addr = address.into();
        let name = name.into();
        let id = format!("{protocol:?}:{}", addr);
        Self {
            id,
            name,
            protocol,
            address: addr,
            model: None,
        }
    }
}

/// Parses user-entered URI or address such as `chromecast://192.168.1.15`, `dlna://192.168.1.20:1400`, `fcast://192.168.1.30`.
pub fn parse_cast_address(input: &str) -> Option<CastDevice> {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("chromecast://") {
        Some(CastDevice::new(
            format!("Chromecast ({rest})"),
            CastProtocol::Chromecast,
            rest,
        ))
    } else if let Some(rest) = trimmed.strip_prefix("dlna://") {
        Some(CastDevice::new(
            format!("DLNA TV / Speaker ({rest})"),
            CastProtocol::Dlna,
            rest,
        ))
    } else if let Some(rest) = trimmed.strip_prefix("fcast://") {
        Some(CastDevice::new(
            format!("FCast Receiver ({rest})"),
            CastProtocol::FCast,
            rest,
        ))
    } else if !trimmed.is_empty() {
        let protocol = if trimmed.contains("46899") {
            CastProtocol::FCast
        } else if trimmed.contains("1400") || trimmed.contains("49152") {
            CastProtocol::Dlna
        } else {
            CastProtocol::Chromecast
        };
        Some(CastDevice::new(
            format!("{} ({trimmed})", protocol.label()),
            protocol,
            trimmed,
        ))
    } else {
        None
    }
}

/// SSDP discovery for DLNA / UPnP MediaRenderers on local network using standard UDP sockets.
pub fn discover_ssdp_devices(timeout: Duration) -> Vec<CastDevice> {
    let mut devices = Vec::new();
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let Ok(socket) = UdpSocket::bind(bind_addr) else {
        return devices;
    };
    let _ = socket.set_broadcast(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(300)));

    let msg = concat!(
        "M-SEARCH * HTTP/1.1\r\n",
        "HOST: 239.255.255.250:1900\r\n",
        "MAN: \"ssdp:discover\"\r\n",
        "MX: 2\r\n",
        "ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n"
    );

    let target: SocketAddr = "239.255.255.250:1900".parse().unwrap();
    let _ = socket.send_to(msg.as_bytes(), target);

    let mut buf = [0u8; 2048];
    let start = Instant::now();

    while Instant::now().duration_since(start) < timeout {
        if let Ok((len, peer)) = socket.recv_from(&mut buf) {
            let response = String::from_utf8_lossy(&buf[..len]);
            let mut location = None;
            for line in response.lines() {
                if let Some(loc) = line.strip_prefix("LOCATION:") {
                    location = Some(loc.trim().to_string());
                } else if let Some(loc) = line.strip_prefix("location:") {
                    location = Some(loc.trim().to_string());
                }
            }
            let dev_addr = location.unwrap_or_else(|| peer.to_string());
            let dev_id = format!("dlna:{}", peer.ip());
            if !devices.iter().any(|d: &CastDevice| d.id == dev_id) {
                devices.push(CastDevice {
                    id: dev_id,
                    name: format!("DLNA Device ({})", peer.ip()),
                    protocol: CastProtocol::Dlna,
                    address: dev_addr,
                    model: Some("MediaRenderer".to_string()),
                });
            }
        }
    }

    devices
}
