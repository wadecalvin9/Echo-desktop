use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;

pub const ORIGIN_YOUTUBE_MUSIC: &str = "https://music.youtube.com";
pub const REFERER_YOUTUBE_MUSIC: &str = "https://music.youtube.com/";
pub const API_URL_YOUTUBE_MUSIC: &str = "https://music.youtube.com/youtubei/v1/";

#[derive(Debug, Clone)]
pub struct YouTubeClientProfile {
    pub id: &'static str,
    pub client_name: &'static str,
    pub client_version: &'static str,
    pub client_id: &'static str,
    pub user_agent: &'static str,
    pub os_version: Option<&'static str>,
    pub os_name: Option<&'static str>,
    pub device_make: Option<&'static str>,
    pub device_model: Option<&'static str>,
    pub login_supported: bool,
}

impl YouTubeClientProfile {
    pub const WEB_REMIX: Self = Self {
        id: "WEB_REMIX",
        client_name: "WEB_REMIX",
        client_version: "1.20250310.01.00",
        client_id: "67",
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0",
        os_version: None,
        os_name: None,
        device_make: None,
        device_model: None,
        login_supported: true,
    };

    pub const VISIONOS: Self = Self {
        id: "VISIONOS",
        client_name: "VISIONOS",
        client_version: "1.02",
        client_id: "101",
        user_agent: "com.google.ios.youtube/1.02 (RealityDevice17,1; U; visionOS 2_3)",
        os_version: Some("2.3.21O5565d"),
        os_name: Some("visionOS"),
        device_make: Some("Apple"),
        device_model: Some("RealityDevice17,1"),
        login_supported: false,
    };

    pub const VISIONOS_1_04: Self = Self {
        id: "VISIONOS_1_04",
        client_name: "VISIONOS",
        client_version: "1.04",
        client_id: "101",
        user_agent: "com.google.visionos.youtube/1.04(RealityDevice17,1; U; CPU visionOS 26_6_0 like Mac OS X; US)",
        os_version: Some("26.6.0.23O770"),
        os_name: Some("visionOS"),
        device_make: Some("Apple"),
        device_model: Some("RealityDevice17,1"),
        login_supported: false,
    };

    pub const VISIONOS_SAFARI: Self = Self {
        id: "VISIONOS_SAFARI",
        client_name: "VISIONOS",
        client_version: "1.02",
        client_id: "101",
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
        os_version: Some("26.5.23O471"),
        os_name: Some("visionOS"),
        device_make: Some("Apple"),
        device_model: Some("RealityDevice17,1"),
        login_supported: false,
    };

    pub const IOS: Self = Self {
        id: "IOS",
        client_name: "IOS",
        client_version: "20.10.4",
        client_id: "5",
        user_agent: "com.google.ios.youtube/20.10.4 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)",
        os_version: Some("18.3.2.22D82"),
        os_name: Some("iPhone OS"),
        device_make: Some("Apple"),
        device_model: Some("iPhone16,2"),
        login_supported: false,
    };

    pub const ANDROID_VR: Self = Self {
        id: "ANDROID_VR",
        client_name: "ANDROID_VR",
        client_version: "1.61.48",
        client_id: "28",
        user_agent: "com.google.android.apps.youtube.vr.oculus/1.61.48 (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip",
        os_version: Some("12L"),
        os_name: Some("Android"),
        device_make: Some("Oculus"),
        device_model: Some("Quest 3"),
        login_supported: false,
    };

    pub const PLAYER_FALLBACK_CHAIN: [Self; 5] = [
        Self::VISIONOS,
        Self::VISIONOS_1_04,
        Self::VISIONOS_SAFARI,
        Self::ANDROID_VR,
        Self::IOS,
    ];

    pub fn context_json(
        &self,
        gl: &str,
        hl: &str,
        visitor_data: Option<&str>,
        data_sync_id: Option<&str>,
    ) -> Value {
        let mut client = serde_json::Map::new();
        client.insert("clientName".into(), json!(self.client_name));
        client.insert("clientVersion".into(), json!(self.client_version));
        client.insert("gl".into(), json!(gl));
        client.insert("hl".into(), json!(hl));
        if let Some(v) = self.os_version {
            client.insert("osVersion".into(), json!(v));
        }
        if let Some(v) = self.os_name {
            client.insert("osName".into(), json!(v));
        }
        if let Some(v) = self.device_make {
            client.insert("deviceMake".into(), json!(v));
        }
        if let Some(v) = self.device_model {
            client.insert("deviceModel".into(), json!(v));
        }
        if let Some(v) = visitor_data.filter(|s| !s.is_empty()) {
            client.insert("visitorData".into(), json!(v));
        }

        let mut user = serde_json::Map::new();
        user.insert("lockedSafetyMode".into(), json!(false));
        if self.login_supported {
            if let Some(ds) = data_sync_id.filter(|s| !s.is_empty()) {
                user.insert("onBehalfOfUser".into(), json!(ds));
            }
        }

        json!({
            "client": client,
            "request": {
                "internalExperimentFlags": [],
                "useSsl": true
            },
            "user": user
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YouTubeLocale {
    pub gl: String,
    pub hl: String,
}

pub fn parse_cookie_map(cookie: &str) -> HashMap<String, String> {
    cookie
        .split(';')
        .filter_map(|part| {
            let (k, v) = part.trim().split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

pub fn extract_sapisid_from_cookie_map(map: &HashMap<String, String>) -> Option<&str> {
    map.get("SAPISID")
        .or_else(|| map.get("__Secure-3PAPISID"))
        .or_else(|| map.get("__Secure-1PAPISID"))
        .or_else(|| map.get("APISID"))
        .map(|s| s.trim().trim_matches('"'))
}

pub fn sapisid_auth_header(sapisid: &str, unix_secs: i64) -> String {
    let input = format!("{unix_secs} {sapisid} {ORIGIN_YOUTUBE_MUSIC}");
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("SAPISIDHASH {unix_secs}_{digest} SAPISID1PHASH {unix_secs}_{digest} SAPISID3PHASH {unix_secs}_{digest}")
}

