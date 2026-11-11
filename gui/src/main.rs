#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const BRAND_PINK: egui::Color32 = egui::Color32::from_rgb(255, 45, 111);
const BRAND_ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 154, 106);
const BRAND_BUTTON: egui::Color32 = egui::Color32::from_rgb(255, 158, 184);
const BRAND_BUTTON_HOVER: egui::Color32 = egui::Color32::from_rgb(255, 184, 203);
const BRAND_BUTTON_ACTIVE: egui::Color32 = egui::Color32::from_rgb(255, 136, 168);
const BUTTON_TEXT: egui::Color32 = egui::Color32::from_rgb(52, 18, 32);

const TEXT_MAIN: egui::Color32 = egui::Color32::from_rgb(247, 244, 250);
const TEXT_FADE: egui::Color32 = egui::Color32::from_rgb(214, 208, 220);
const LINK_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 188, 150);

const DUB_VOICES: &[(&str, &str)] = &[
    ("vi-VN-HoaiMyNeural", "Nữ — Hoài My"),
    ("vi-VN-NamMinhNeural", "Nam — Nam Minh"),
];

const TTS_PROVIDERS: &[(&str, &str)] = &[
    ("edge", "Edge TTS (online)"),
    ("vieneu", "VieNeu-TTS (local, 48kHz)"),
    ("zerotts", "ZeroTTS (local, 8 giọng)"),
    ("gtts", "Google TTS — gTTS (online)"),
    ("elevenlabs", "ElevenLabs (API key)"),
    ("openai", "OpenAI TTS (API key)"),
    ("google", "Google Cloud TTS (service account)"),
    ("azure", "Azure Speech (API key)"),
];

const ZEROTTS_VOICES: &[(&str, &str)] = &[
    ("maichi", "Mai Chi — nữ, kể chuyện"),
    ("baotrang", "Bảo Trang — nữ, tin tức"),
    ("kimoanh", "Kim Oanh — nữ, trung niên"),
    ("hamy", "Hà My — nữ, hoạt hình"),
    ("giahuy", "Gia Huy — nam, trầm ấm"),
    ("huuduc", "Hữu Đức — nam, lớn tuổi"),
    ("quangminh", "Quang Minh — nam, tin tức"),
    ("tiendat", "Tiến Đạt — nam, bình luận"),
];

const OPENAI_VOICES: &[&str] = &[
    "alloy", "ash", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer",
];

const GOOGLE_VOICES: &[&str] = &[
    "vi-VN-Wavenet-A",
    "vi-VN-Wavenet-B",
    "vi-VN-Wavenet-C",
    "vi-VN-Wavenet-D",
    "vi-VN-Standard-A",
    "vi-VN-Standard-B",
    "vi-VN-Standard-C",
    "vi-VN-Standard-D",
];

const AZURE_VOICES: &[&str] = &["vi-VN-HoaiMyNeural", "vi-VN-NamMinhNeural"];

const ELEVENLABS_DEFAULT_VOICE: &str = "21m00Tcm4TlvDq8ikWAM";

const VIENEU_VOICES: &[&str] = &[
    "Adam bựa",
    "Trúc Ly",
    "Thiện Minh",
    "Mai Anh",
    "Hải Đăng",
    "Thùy Dung",
    "Thiền Tâm Đức",
    "Ngọc Huyền",
    "Quang Sơn",
    "Ngọc Trân",
    "Minh Đức",
    "Phạm Tuyên",
    "Thái Sơn",
    "Xuân Vĩnh",
    "Thanh Bình",
    "Ngọc Linh",
    "Đoan Trang",
    "Thục Đoan",
    "Minh Triết",
    "Mỹ Duyên",
    "Quỳnh Anh",
    "Đức Trí",
    "Kim Thanh",
    "Adam",
    "Quốc Tuấn",
];

const PLATFORMS: &[(&str, &str)] = &[
    ("", "Tự động nhận diện"),
    ("dramabox", "DramaBox"),
    ("goodshort", "GoodShort / GoodReels"),
    ("shortmax", "ShortMax"),
    ("netshort", "NetShort / FlickReels"),
    ("freereels", "FreeReels"),
    ("kisskh", "KissKH"),
    ("hongguo", "Hongguo / Fanqie"),
    ("dramawave", "DramaWave"),
    ("raptdrama", "RaptDrama"),
    ("flextv", "FlexTV"),
    ("stardusttv", "StardustTV"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Download,
    Catalog,
    Dubbing,
    Merge,
    Donate,
    Log,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Download => "Tải phim",
            Tab::Catalog => "Danh mục",
            Tab::Dubbing => "Lồng tiếng",
            Tab::Merge => "Ghép tập",
            Tab::Donate => "Donate",
            Tab::Log => "Nhật ký",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Idle,
    Info,
    Download,
    Catalog,
    Dubbing,
    Merging,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct EpisodeInfo {
    #[serde(default)]
    num: u32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    has_stream: bool,
    #[serde(default)]
    subs: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SeriesInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    series_id: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    episode_count: usize,
    #[serde(default)]
    accessible_count: usize,
    #[serde(default)]
    episodes: Vec<EpisodeInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CatalogItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    episodes: usize,
    #[serde(default)]
    link: String,
    #[serde(default)]
    platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    output_dir: String,
    concurrency: u32,
    save_vtt: bool,
    platform: String,
    last_url: String,
    catalog_platform: String,
    dub_folder: String,
    dub_output: String,
    dub_provider: String,
    dub_voice: String,
    dub_original_enabled: bool,
    dub_original_volume: u32,
    dub_tts_volume: u32,
    dub_rate: i32,
    dub_use_sidecar: bool,
    dub_overwrite: bool,
    dub_ocr_only: bool,
    dub_ocr_mode: String,
    dub_soft_timing: bool,
    dub_natural_voice: bool,
    dub_keep_temp: bool,
    tts_api_key: String,
    tts_region: String,
    tts_credentials: String,
    merge_folder: String,
    merge_output: String,
    merge_overwrite: bool,
    merge_reencode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: "downloads".to_owned(),
            concurrency: 3,
            save_vtt: false,
            platform: String::new(),
            last_url: String::new(),
            catalog_platform: "dramabox".to_owned(),
            dub_folder: String::new(),
            dub_output: String::new(),
            dub_provider: "edge".to_owned(),
            dub_voice: DUB_VOICES[0].0.to_owned(),
            dub_original_enabled: true,
            dub_original_volume: 15,
            dub_tts_volume: 100,
            dub_rate: 0,
            dub_use_sidecar: true,
            dub_overwrite: false,
            dub_ocr_only: false,
            dub_ocr_mode: "quality".to_owned(),
            dub_soft_timing: false,
            dub_natural_voice: true,
            dub_keep_temp: false,
            tts_api_key: String::new(),
            tts_region: String::new(),
            tts_credentials: String::new(),
            merge_folder: String::new(),
            merge_output: String::new(),
            merge_overwrite: true,
            merge_reencode: false,
        }
    }
}

fn settings_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
            .join("DramaAI Tiktok Downloader")
            .join("settings.json")
    } else {
        project_root().join("settings.json")
    }
}

fn load_settings() -> Settings {
    let path = settings_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_settings_to_disk(settings: &Settings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, text);
    }
}

#[derive(Default)]
struct DownloadState {
    total: usize,
    done: usize,
    failed: usize,
    current: Option<u32>,
    ep_progress: HashMap<u32, f32>,
    finished: bool,
}

struct App {
    tab: Tab,
    url: String,
    platform: String,
    output_dir: String,
    concurrency: u32,
    save_vtt: bool,

    series: Option<SeriesInfo>,
    loaded_target: String,
    selected: HashSet<u32>,
    ep_filter: String,
    range_input: String,
    show_filters: bool,

    busy: Busy,
    status: String,
    error: Option<String>,
    log: VecDeque<String>,

    dl: Option<DownloadState>,

    catalog_platform: String,
    catalog_query: String,
    catalog_page: i32,
    catalog_items: Vec<CatalogItem>,
    catalog_total: usize,

    dub_folder: String,
    dub_output: String,
    dub_provider: String,
    dub_voice: String,
    dub_original_enabled: bool,
    dub_original_volume: u32,
    dub_tts_volume: u32,
    dub_rate: i32,
    dub_use_sidecar: bool,
    dub_overwrite: bool,
    dub_ocr_only: bool,
    dub_ocr_mode: String,
    dub_soft_timing: bool,
    dub_natural_voice: bool,
    dub_keep_temp: bool,
    tts_api_key: String,
    tts_region: String,
    tts_credentials: String,
    dub_video_count: usize,
    dub_total: usize,
    dub_done: usize,
    dub_failed: usize,
    dub_current: String,
    dub_progress: f32,
    dub_stage: String,
    dub_finished: bool,

    merge_folder: String,
    merge_output: String,
    merge_video_count: usize,
    merge_overwrite: bool,
    merge_reencode: bool,
    merge_total: usize,
    merge_progress: f32,
    merge_stage: String,
    merge_output_path: String,
    merge_finished: bool,

    donate_texture: Option<egui::TextureHandle>,

    child: Option<Child>,
    rx: Option<Receiver<(u64, Value)>>,
    generation: u64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        configure_style(&cc.egui_ctx);
        let mut app = Self::base();
        if let Ok(start_url) = std::env::var("DRAMA_GUI_START_URL") {
            let start_url = start_url.trim().to_owned();
            if !start_url.is_empty() {
                app.url = start_url;
                app.start_info(&cc.egui_ctx);
            }
        }
        app
    }

    fn base() -> Self {
        let mut log = VecDeque::new();
        log.push_back(format!(
            "{} Sẵn sàng. Dán link phim và bấm \"Tải thông tin\".",
            now_hms()
        ));

        let tab = match std::env::var("DRAMA_GUI_START_TAB")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "catalog" => Tab::Catalog,
            "dubbing" | "tts" => Tab::Dubbing,
            "merge" | "ghep" => Tab::Merge,
            "donate" => Tab::Donate,
            "log" => Tab::Log,
            _ => Tab::Download,
        };

        let settings = load_settings();
        let output_dir = if settings.output_dir.trim().is_empty() {
            "downloads".to_owned()
        } else {
            settings.output_dir.clone()
        };
        let concurrency = if (1..=8).contains(&settings.concurrency) {
            settings.concurrency
        } else {
            3
        };
        if output_dir != "downloads" {
            log.push_back(format!("{} Thư mục tải đã lưu: {}", now_hms(), output_dir));
        }

        let dub_provider = if TTS_PROVIDERS
            .iter()
            .any(|(id, _)| *id == settings.dub_provider)
        {
            settings.dub_provider.clone()
        } else {
            "edge".to_owned()
        };
        let dub_voice = if is_voice_valid(&dub_provider, &settings.dub_voice) {
            settings.dub_voice.clone()
        } else {
            default_tts_voice(&dub_provider)
        };
        let dub_ocr_mode = if settings.dub_ocr_mode == "fast" {
            "fast".to_owned()
        } else {
            "quality".to_owned()
        };
        let dub_folder = settings.dub_folder.clone();
        let dub_video_count = count_videos(&dub_folder);
        let merge_folder = settings.merge_folder.clone();
        let merge_video_count = count_videos(&merge_folder);
        let catalog_platform = if settings.catalog_platform.trim().is_empty() {
            "dramabox".to_owned()
        } else {
            settings.catalog_platform.clone()
        };

        Self {
            tab,
            url: settings.last_url.clone(),
            platform: settings.platform.clone(),
            output_dir,
            concurrency,
            save_vtt: settings.save_vtt,
            series: None,
            loaded_target: String::new(),
            selected: HashSet::new(),
            ep_filter: String::new(),
            range_input: String::new(),
            show_filters: false,
            busy: Busy::Idle,
            status: "Sẵn sàng".to_owned(),
            error: None,
            log,
            dl: None,
            catalog_platform,
            catalog_query: String::new(),
            catalog_page: 1,
            catalog_items: Vec::new(),
            catalog_total: 0,
            dub_folder,
            dub_output: settings.dub_output.clone(),
            dub_provider,
            dub_voice,
            dub_original_enabled: settings.dub_original_enabled,
            dub_original_volume: settings.dub_original_volume.clamp(0, 100),
            dub_tts_volume: settings.dub_tts_volume.clamp(0, 200),
            dub_rate: settings.dub_rate.clamp(-30, 30),
            dub_use_sidecar: settings.dub_use_sidecar,
            dub_overwrite: settings.dub_overwrite,
            dub_ocr_only: settings.dub_ocr_only,
            dub_ocr_mode,
            dub_soft_timing: settings.dub_soft_timing,
            dub_natural_voice: settings.dub_natural_voice,
            dub_keep_temp: settings.dub_keep_temp,
            tts_api_key: settings.tts_api_key.clone(),
            tts_region: settings.tts_region.clone(),
            tts_credentials: settings.tts_credentials.clone(),
            dub_video_count,
            dub_total: 0,
            dub_done: 0,
            dub_failed: 0,
            dub_current: String::new(),
            dub_progress: 0.0,
            dub_stage: String::new(),
            dub_finished: false,
            merge_folder,
            merge_output: settings.merge_output.clone(),
            merge_video_count,
            merge_overwrite: settings.merge_overwrite,
            merge_reencode: settings.merge_reencode,
            merge_total: 0,
            merge_progress: 0.0,
            merge_stage: String::new(),
            merge_output_path: String::new(),
            merge_finished: false,
            donate_texture: None,
            child: None,
            rx: None,
            generation: 0,
        }
    }

    fn push_log(&mut self, message: impl Into<String>) {
        self.log
            .push_back(format!("{} {}", now_hms(), message.into()));
        while self.log.len() > 500 {
            self.log.pop_front();
        }
    }

    fn save_settings(&self) {
        save_settings_to_disk(&Settings {
            output_dir: self.output_dir.clone(),
            concurrency: self.concurrency,
            save_vtt: self.save_vtt,
            platform: self.platform.clone(),
            last_url: self.url.clone(),
            catalog_platform: self.catalog_platform.clone(),
            dub_folder: self.dub_folder.clone(),
            dub_output: self.dub_output.clone(),
            dub_provider: self.dub_provider.clone(),
            dub_voice: self.dub_voice.clone(),
            dub_original_enabled: self.dub_original_enabled,
            dub_original_volume: self.dub_original_volume,
            dub_tts_volume: self.dub_tts_volume,
            dub_rate: self.dub_rate,
            dub_use_sidecar: self.dub_use_sidecar,
            dub_overwrite: self.dub_overwrite,
            dub_ocr_only: self.dub_ocr_only,
            dub_ocr_mode: self.dub_ocr_mode.clone(),
            dub_soft_timing: self.dub_soft_timing,
            dub_natural_voice: self.dub_natural_voice,
            dub_keep_temp: self.dub_keep_temp,
            tts_api_key: self.tts_api_key.clone(),
            tts_region: self.tts_region.clone(),
            tts_credentials: self.tts_credentials.clone(),
            merge_folder: self.merge_folder.clone(),
            merge_output: self.merge_output.clone(),
            merge_overwrite: self.merge_overwrite,
            merge_reencode: self.merge_reencode,
        });
    }

    fn platform_hint(&self) -> Value {
        if self.platform.is_empty() {
            Value::Null
        } else {
            json!(self.platform)
        }
    }

    fn start_info(&mut self, ctx: &egui::Context) {
        if self.busy != Busy::Idle {
            return;
        }
        let target = self.url.trim().to_owned();
        if target.is_empty() {
            self.error = Some("Chưa nhập link phim hoặc Series ID.".to_owned());
            return;
        }
        self.save_settings();

        self.error = None;
        self.series = None;
        self.loaded_target = target.clone();
        self.selected.clear();
        self.dl = None;
        self.busy = Busy::Info;
        self.status = "Đang tải thông tin phim...".to_owned();
        self.push_log(format!("Tải thông tin: {target}"));

        let request = json!({
            "cmd": "info",
            "target": target,
            "platform": self.platform_hint(),
        });
        if let Err(error) = self.spawn_bridge(ctx, request) {
            self.busy = Busy::Idle;
            self.error = Some(error.clone());
            self.push_log(format!("LỖI: {error}"));
        }
    }

    fn start_download(&mut self, ctx: &egui::Context) {
        if self.busy != Busy::Idle || self.series.is_none() {
            return;
        }
        if self.selected.is_empty() {
            self.error = Some("Chưa chọn tập nào.".to_owned());
            return;
        }

        let mut episodes: Vec<u32> = self.selected.iter().copied().collect();
        episodes.sort_unstable();
        let target = if self.loaded_target.is_empty() {
            self.url.trim().to_owned()
        } else {
            self.loaded_target.clone()
        };
        let output = if self.output_dir.trim().is_empty() {
            "./downloads".to_owned()
        } else {
            self.output_dir.trim().to_owned()
        };
        self.save_settings();

        self.error = None;
        self.dl = Some(DownloadState {
            total: episodes.len(),
            ..Default::default()
        });
        self.busy = Busy::Download;
        self.status = format!("Đang tải {} tập...", episodes.len());
        self.push_log(format!(
            "Bắt đầu tải {} tập vào {} ({} luồng)",
            episodes.len(),
            output,
            self.concurrency
        ));

        let request = json!({
            "cmd": "download",
            "target": target,
            "platform": self.platform_hint(),
            "output": output,
            "concurrency": self.concurrency,
            "save_vtt": self.save_vtt,
            "episodes": episodes,
        });
        if let Err(error) = self.spawn_bridge(ctx, request) {
            self.busy = Busy::Idle;
            self.error = Some(error.clone());
            self.dl = None;
            self.push_log(format!("LỖI: {error}"));
        }
    }

    fn start_catalog(&mut self, ctx: &egui::Context) {
        if self.busy != Busy::Idle {
            return;
        }
        if self.catalog_platform.is_empty() {
            self.error = Some("Chọn nền tảng để duyệt danh mục.".to_owned());
            return;
        }

        self.save_settings();
        self.error = None;
        self.catalog_items.clear();
        self.catalog_total = 0;
        self.busy = Busy::Catalog;
        self.status = format!(
            "Đang tải danh mục {} (trang {})...",
            platform_label(&self.catalog_platform),
            self.catalog_page
        );

        let query = self.catalog_query.trim();
        let request = json!({
            "cmd": "catalog",
            "platform": self.catalog_platform,
            "page": self.catalog_page,
            "query": if query.is_empty() { Value::Null } else { json!(query) },
        });
        if let Err(error) = self.spawn_bridge(ctx, request) {
            self.busy = Busy::Idle;
            self.error = Some(error.clone());
            self.push_log(format!("LỖI: {error}"));
        }
    }

    fn start_dubbing(&mut self, ctx: &egui::Context) {
        if self.busy != Busy::Idle {
            return;
        }
        let folder = self.dub_folder.trim().to_owned();
        if !std::path::Path::new(&folder).is_dir() {
            self.error = Some("Chọn thư mục chứa video trước.".to_owned());
            return;
        }
        let count = count_videos(&folder);
        if count == 0 {
            self.error = Some("Không tìm thấy video trong thư mục.".to_owned());
            return;
        }
        let output = self.dub_output.trim().to_owned();
        let original_volume = if self.dub_original_enabled {
            self.dub_original_volume
        } else {
            0
        };

        self.save_settings();
        self.error = None;
        self.dub_total = count;
        self.dub_done = 0;
        self.dub_failed = 0;
        self.dub_current.clear();
        self.dub_progress = 0.0;
        self.dub_stage.clear();
        self.dub_finished = false;
        self.busy = Busy::Dubbing;
        self.status = format!("Đang lồng tiếng 0/{count} video...");
        self.push_log(format!(
            "Bắt đầu lồng tiếng {count} video từ {folder} ({} · OCR: {}, gốc {}%, TTS {}%)",
            tts_provider_label(&self.dub_provider),
            self.dub_ocr_mode,
            original_volume,
            self.dub_tts_volume
        ));

        let request = json!({
            "cmd": "dub",
            "folder": folder,
            "output": output,
            "voice": self.dub_voice,
            "tts_provider": self.dub_provider,
            "original_volume": original_volume,
            "tts_volume": self.dub_tts_volume,
            "rate": self.dub_rate,
            "use_sidecar": self.dub_use_sidecar,
            "overwrite": self.dub_overwrite,
            "ocr_only": self.dub_ocr_only,
            "ocr_mode": self.dub_ocr_mode,
            "soft_timing": self.dub_soft_timing,
            "natural_voice": self.dub_natural_voice,
            "keep_temp": self.dub_keep_temp,
            "tts_api_key": self.tts_api_key,
            "tts_region": self.tts_region,
            "tts_credentials": self.tts_credentials,
        });
        if let Err(error) = self.spawn_bridge(ctx, request) {
            self.busy = Busy::Idle;
            self.error = Some(error.clone());
            self.push_log(format!("LỖI: {error}"));
        }
    }

    fn cancel(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if self.busy == Busy::Dubbing {
            self.dub_finished = true;
            self.dub_stage = "Đã hủy".to_owned();
        }
        if self.busy == Busy::Merging {
            self.merge_finished = true;
            self.merge_stage = "Đã hủy".to_owned();
        }
        self.busy = Busy::Idle;
        self.status = "Đã hủy".to_owned();
        self.error = None;
        if let Some(dl) = &mut self.dl {
            dl.finished = true;
        }
        self.push_log("Đã hủy tiến trình. File .part có thể còn lại trong thư mục tải.");
    }

    fn start_merge(&mut self, ctx: &egui::Context) {
        if self.busy != Busy::Idle {
            return;
        }
        let folder = self.merge_folder.trim().to_owned();
        if !std::path::Path::new(&folder).is_dir() {
            self.error = Some("Chọn thư mục chứa các tập phim trước.".to_owned());
            return;
        }
        let count = count_videos(&folder);
        if count == 0 {
            self.error = Some("Không tìm thấy video trong thư mục.".to_owned());
            return;
        }

        self.save_settings();
        self.error = None;
        self.merge_total = count;
        self.merge_progress = 0.0;
        self.merge_stage = "Bắt đầu".to_owned();
        self.merge_output_path.clear();
        self.merge_finished = false;
        self.busy = Busy::Merging;
        self.status = format!("Đang ghép {count} video...");
        self.push_log(format!(
            "Bắt đầu ghép {count} video từ {folder} ({})",
            if self.merge_reencode {
                "có re-encode dự phòng"
            } else {
                "copy nhanh"
            }
        ));

        let request = json!({
            "cmd": "merge",
            "folder": folder,
            "output": self.merge_output.trim(),
            "overwrite": self.merge_overwrite,
            "reencode": self.merge_reencode,
        });
        if let Err(error) = self.spawn_bridge(ctx, request) {
            self.busy = Busy::Idle;
            self.error = Some(error.clone());
            self.push_log(format!("LỖI: {error}"));
        }
    }

    fn spawn_bridge(&mut self, ctx: &egui::Context, request: Value) -> Result<(), String> {
        if let Some(mut old) = self.child.take() {
            let _ = old.kill();
            let _ = old.wait();
        }

        let root = project_root();
        let script = root.join("drama_downloader").join("bridge.py");
        if !script.is_file() {
            return Err(format!(
                "Không tìm thấy bridge Python tại {}",
                script.display()
            ));
        }

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let mut command = Command::new(python_exe());
        command
            .arg("-u")
            .arg(&script)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1");

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Không chạy được Python ({}): {error}. Đặt biến DRAMA_PYTHON nếu cần.",
                python_exe()
            )
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            let payload = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
            stdin
                .write_all(&payload)
                .map_err(|error| error.to_string())?;
            // stdin is dropped here so Python receives EOF.
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Không đọc được stdout của Python.".to_owned())?;
        let stderr = child.stderr.take();

        let (tx, rx) = mpsc::channel::<(u64, Value)>();

        let ctx_out = ctx.clone();
        let tx_out = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let value = serde_json::from_str::<Value>(line)
                    .unwrap_or_else(|_| json!({"event": "log", "message": line}));
                let _ = tx_out.send((generation, value));
                ctx_out.request_repaint();
            }
            let _ = tx_out.send((generation, json!({"event": "process_exit"})));
            ctx_out.request_repaint();
        });

        if let Some(stderr) = stderr {
            let ctx_err = ctx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let _ = tx.send((generation, json!({"event": "stderr", "message": line})));
                    ctx_err.request_repaint();
                }
            });
        }

        self.child = Some(child);
        self.rx = Some(rx);
        Ok(())
    }

    fn poll_events(&mut self) {
        let mut batch = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok((generation, value)) = rx.try_recv() {
                if generation == self.generation {
                    batch.push(value);
                }
            }
        }
        for value in batch {
            self.handle_event(value);
        }
    }

    fn handle_event(&mut self, value: Value) {
        let event = value
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match event {
            "series" => {
                let Some(series_value) = value.get("series") else {
                    return;
                };
                let Ok(series) = serde_json::from_value::<SeriesInfo>(series_value.clone()) else {
                    self.error = Some("Dữ liệu series không hợp lệ.".to_owned());
                    return;
                };

                if self.busy == Busy::Info {
                    self.selected = series
                        .episodes
                        .iter()
                        .filter(|ep| ep.has_stream && !ep.locked)
                        .map(|ep| ep.num)
                        .collect();
                    self.busy = Busy::Idle;
                    self.status = format!("Đã tải thông tin: {}", series.name);
                    self.push_log(format!(
                        "{} — {} tập, {} khả dụng",
                        series.name, series.episode_count, series.accessible_count
                    ));
                }
                self.series = Some(series);
            }
            "start" => {
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(dl) = &mut self.dl {
                    dl.total = total;
                    dl.done = 0;
                    dl.failed = 0;
                    dl.current = None;
                    dl.ep_progress.clear();
                    dl.finished = false;
                }
                if let Some(series_dir) = value.get("series_dir").and_then(Value::as_str) {
                    self.push_log(format!("Thư mục phim: {series_dir}"));
                }
                self.status = format!("Đang tải 0/{total} tập...");
            }
            "episode_start" => {
                let num = value.get("num").and_then(Value::as_u64).unwrap_or(0) as u32;
                if let Some(dl) = &mut self.dl {
                    dl.current = Some(num);
                    dl.ep_progress.entry(num).or_insert(0.0);
                }
                self.push_log(format!("Bắt đầu EP{num:03}"));
            }
            "progress" => {
                let num = value.get("num").and_then(Value::as_u64).unwrap_or(0) as u32;
                let percent =
                    value.get("percent").and_then(Value::as_f64).unwrap_or(0.0) as f32 / 100.0;
                if let Some(dl) = &mut self.dl {
                    dl.current = Some(num);
                    dl.ep_progress.insert(num, percent.clamp(0.0, 1.0));
                }
            }
            "episode_done" => {
                let num = value.get("num").and_then(Value::as_u64).unwrap_or(0) as u32;
                if let Some(dl) = &mut self.dl {
                    dl.done += 1;
                    dl.ep_progress.insert(num, 1.0);
                    if dl.current == Some(num) {
                        dl.current = None;
                    }
                    self.status = format!("Đang tải {}/{} tập...", dl.done, dl.total);
                }
                let path = value.get("path").and_then(Value::as_str).unwrap_or("");
                self.push_log(format!("Xong EP{num:03}: {path}"));
            }
            "episode_error" => {
                let num = value.get("num").and_then(Value::as_u64).unwrap_or(0) as u32;
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("lỗi không xác định");
                if let Some(dl) = &mut self.dl {
                    dl.failed += 1;
                    dl.ep_progress.insert(num, 0.0);
                    if dl.current == Some(num) {
                        dl.current = None;
                    }
                }
                self.push_log(format!("LỖI EP{num:03}: {message}"));
            }
            "finished" => {
                let ok = value.get("ok").and_then(Value::as_u64).unwrap_or(0) as usize;
                let failed = value.get("failed").and_then(Value::as_u64).unwrap_or(0) as usize;
                let elapsed = value.get("elapsed").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(dl) = &mut self.dl {
                    dl.done = ok;
                    dl.failed = failed;
                    dl.current = None;
                    dl.finished = true;
                }
                self.busy = Busy::Idle;
                self.status = format!("Hoàn tất: {ok} thành công, {failed} lỗi ({elapsed:.0}s)");
                self.push_log(format!(
                    "Hoàn tất: {ok} thành công, {failed} lỗi, {elapsed:.1} giây"
                ));
            }
            "catalog" => {
                let items = value
                    .get("items")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new()));
                self.catalog_items =
                    serde_json::from_value::<Vec<CatalogItem>>(items).unwrap_or_default();
                self.catalog_total =
                    value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                self.busy = Busy::Idle;
                self.status = format!("Danh mục: {} kết quả", self.catalog_total);
                self.push_log(format!(
                    "Danh mục {}: {} kết quả (trang {})",
                    platform_label(&self.catalog_platform),
                    self.catalog_total,
                    self.catalog_page
                ));
            }
            "dub_start" => {
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                self.dub_total = total;
                self.dub_done = 0;
                self.dub_failed = 0;
                self.dub_current.clear();
                self.dub_progress = 0.0;
                self.dub_stage = "Bắt đầu".to_owned();
                self.dub_finished = false;
                if let Some(output) = value.get("output_dir").and_then(Value::as_str) {
                    if self.dub_output.trim().is_empty() {
                        self.dub_output = output.to_owned();
                    }
                    self.push_log(format!("Xuất lồng tiếng vào: {output}"));
                }
                self.status = format!("Đang lồng tiếng 0/{total} video...");
            }
            "dub_video_start" => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                let name = value.get("name").and_then(Value::as_str).unwrap_or("");
                self.dub_current = name.to_owned();
                self.dub_stage = "Đang xử lý".to_owned();
                self.status = format!("Đang xử lý {index}/{total}: {name}");
                self.push_log(format!("[{index}/{total}] {name}"));
            }
            "dub_progress" => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let percent = value.get("percent").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                let message = value.get("message").and_then(Value::as_str).unwrap_or("");
                let total = self.dub_total.max(1) as f32;
                self.dub_progress =
                    (((index.saturating_sub(1)) as f32 + percent / 100.0) / total).clamp(0.0, 1.0);
                self.dub_stage = message.to_owned();
                let stage = value
                    .get("stage")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_uppercase();
                if !message.is_empty() {
                    self.status =
                        format!("Lồng tiếng {index}/{} — {stage}: {message}", self.dub_total);
                }
            }
            "dub_video_done" => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let name = value.get("name").and_then(Value::as_str).unwrap_or("");
                let skipped = value
                    .get("skipped")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.dub_done += 1;
                self.dub_progress = (index as f32 / self.dub_total.max(1) as f32).clamp(0.0, 1.0);
                self.dub_current.clear();
                self.dub_stage.clear();
                if skipped {
                    self.push_log(format!("Bỏ qua (đã có): {name}"));
                } else {
                    self.push_log(format!("Xong: {name}"));
                }
            }
            "dub_video_error" => {
                let name = value.get("name").and_then(Value::as_str).unwrap_or("");
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("lỗi không xác định");
                self.dub_failed += 1;
                self.dub_current.clear();
                self.push_log(format!("LỖI lồng tiếng {name}: {message}"));
            }
            "dub_finished" => {
                let done = value.get("done").and_then(Value::as_u64).unwrap_or(0) as usize;
                let failed = value.get("failed").and_then(Value::as_u64).unwrap_or(0) as usize;
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                let elapsed = value.get("elapsed").and_then(Value::as_f64).unwrap_or(0.0);
                self.dub_done = done;
                self.dub_failed = failed;
                self.dub_total = total;
                self.dub_progress = 1.0;
                self.dub_current.clear();
                self.dub_stage = "Hoàn tất".to_owned();
                self.dub_finished = true;
                self.busy = Busy::Idle;
                self.status =
                    format!("Lồng tiếng xong: {done}/{total} video, {failed} lỗi ({elapsed:.0}s)");
                self.push_log(format!(
                    "Lồng tiếng hoàn tất: {done}/{total} video, {failed} lỗi, {elapsed:.1} giây"
                ));
            }
            "merge_start" => {
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                let output = value.get("output").and_then(Value::as_str).unwrap_or("");
                self.merge_total = total;
                self.merge_progress = 0.0;
                self.merge_stage = "Đang ghép".to_owned();
                self.merge_finished = false;
                self.merge_output_path = output.to_owned();
                if self.merge_output.trim().is_empty() && !output.is_empty() {
                    self.merge_output = output.to_owned();
                }
                self.status = format!("Đang ghép {total} video...");
                if !output.is_empty() {
                    self.push_log(format!("File full: {output}"));
                }
            }
            "merge_progress" => {
                let percent = value.get("percent").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                let message = value.get("message").and_then(Value::as_str).unwrap_or("");
                self.merge_progress = (percent / 100.0).clamp(0.0, 1.0);
                if !message.is_empty() {
                    self.merge_stage = message.to_owned();
                    self.status = format!("Ghép tập: {message}");
                }
            }
            "merge_done" => {
                let output = value.get("output").and_then(Value::as_str).unwrap_or("");
                let size = value.get("size").and_then(Value::as_u64).unwrap_or(0);
                let duration = value.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
                let elapsed = value.get("elapsed").and_then(Value::as_f64).unwrap_or(0.0);
                self.merge_progress = 1.0;
                if !output.is_empty() {
                    self.merge_output_path = output.to_owned();
                }
                self.push_log(format!(
                    "Đã ghép: {output} ({:.1} MB, {:.0} giây video, {:.1}s)",
                    size as f64 / 1_048_576.0,
                    duration,
                    elapsed
                ));
            }
            "merge_error" => {
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("lỗi không xác định");
                self.push_log(format!("LỖI ghép tập: {message}"));
            }
            "merge_finished" => {
                let elapsed = value.get("elapsed").and_then(Value::as_f64).unwrap_or(0.0);
                self.merge_finished = true;
                self.merge_stage = "Hoàn tất".to_owned();
                self.busy = Busy::Idle;
                self.status = format!("Ghép xong ({elapsed:.1}s)");
                self.push_log(format!("Ghép tập hoàn tất trong {elapsed:.1} giây."));
            }
            "error" => {
                let was_dubbing = self.busy == Busy::Dubbing;
                let was_merging = self.busy == Busy::Merging;
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Lỗi không xác định")
                    .to_owned();
                self.error = Some(message.clone());
                self.busy = Busy::Idle;
                self.status = "Lỗi".to_owned();
                if was_dubbing {
                    self.dub_finished = true;
                    self.dub_stage = "Lỗi".to_owned();
                    self.dub_current.clear();
                }
                if was_merging {
                    self.merge_finished = true;
                    self.merge_stage = "Lỗi".to_owned();
                }
                self.push_log(format!("LỖI: {message}"));
                if let Some(traceback) = value.get("traceback").and_then(Value::as_str) {
                    for line in traceback.lines().take(25) {
                        self.push_log(format!("  {line}"));
                    }
                }
                if let Some(dl) = &mut self.dl {
                    dl.finished = true;
                }
            }
            "stderr" => {
                if let Some(message) = value.get("message").and_then(Value::as_str) {
                    self.push_log(format!("[python] {message}"));
                }
            }
            "log" => {
                if let Some(message) = value.get("message").and_then(Value::as_str) {
                    self.push_log(message);
                }
            }
            "process_exit" => {
                if let Some(mut child) = self.child.take() {
                    let _ = child.wait();
                }
                if self.busy != Busy::Idle {
                    if let Some(dl) = &mut self.dl {
                        dl.finished = true;
                    }
                    if self.busy == Busy::Dubbing {
                        self.dub_finished = true;
                        self.dub_stage = "Tiến trình đã dừng".to_owned();
                        self.dub_current.clear();
                    }
                    if self.busy == Busy::Merging {
                        self.merge_finished = true;
                        self.merge_stage = "Tiến trình đã dừng".to_owned();
                    }
                    self.error = Some(
                        "Tiến trình Python đã dừng. Xem tab Nhật ký để biết chi tiết.".to_owned(),
                    );
                    self.busy = Busy::Idle;
                    self.status = "Dừng".to_owned();
                    self.push_log("Tiến trình Python đã dừng bất thường.");
                }
            }
            _ => {}
        }
    }

    fn ui_download(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut start_info = false;
        let mut start_download = false;
        let mut pick_folder = false;
        let mut open_folder = false;
        let mut apply_range = false;
        let mut settings_dirty = false;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("LIÊN KẾT PHIM")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            let url_response = ui.add(
                egui::TextEdit::singleline(&mut self.url)
                    .hint_text(
                        egui::RichText::new("Dán link phim hoặc Series ID...").color(TEXT_FADE),
                    )
                    .desired_width(f32::INFINITY)
                    .text_color(TEXT_MAIN),
            );
            if url_response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                start_info = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Nền tảng:");
                let platform_before = self.platform.clone();
                egui::ComboBox::from_id_salt("platform_combo")
                    .width(220.0)
                    .selected_text(platform_label(&self.platform))
                    .show_ui(ui, |ui| {
                        combo_popup_style(ui);
                        for (id, name) in PLATFORMS {
                            ui.selectable_value(&mut self.platform, (*id).to_owned(), *name);
                        }
                    });
                if self.platform != platform_before {
                    settings_dirty = true;
                }
                if ui
                    .add_enabled(
                        self.busy == Busy::Idle,
                        egui::Button::new("Tải thông tin").fill(BRAND_BUTTON),
                    )
                    .clicked()
                {
                    start_info = true;
                }
                if self.busy == Busy::Info {
                    ui.spinner();
                }
            });
        });

        if let Some(series) = &self.series {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let title = ui.heading(&series.name);
                    if !series.description.is_empty() {
                        title.on_hover_text(&series.description);
                    }
                    ui.label(
                        egui::RichText::new(series.platform.to_uppercase())
                            .small()
                            .weak(),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} tập", series.episode_count));
                    ui.separator();
                    ui.label(format!("{} khả dụng", series.accessible_count));
                    if !series.series_id.is_empty() {
                        ui.separator();
                        ui.monospace(&series.series_id);
                    }
                    if !series.cover.is_empty() {
                        ui.separator();
                        ui.hyperlink_to("Ảnh bìa", &series.cover);
                    }
                });
            });
        }

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("TUỲ CHỌN TẢI")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.horizontal(|ui| {
                ui.label("Thư mục:")
                    .on_hover_text("Đường dẫn được lưu tự động cho lần mở sau");
                let width = (ui.available_width() - 190.0).max(140.0);
                let output_response = ui.add(
                    egui::TextEdit::singleline(&mut self.output_dir)
                        .desired_width(width)
                        .hint_text(egui::RichText::new("./downloads").color(TEXT_FADE))
                        .text_color(TEXT_MAIN),
                );
                if output_response.changed() {
                    settings_dirty = true;
                }
                if ui.button("Chọn...").clicked() {
                    pick_folder = true;
                }
                if ui.button("Mở").clicked() {
                    open_folder = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Luồng:");
                let concurrency_response = ui.add(egui::Slider::new(&mut self.concurrency, 1..=8));
                if concurrency_response.changed() {
                    settings_dirty = true;
                }
                ui.separator();
                let vtt_response = ui.checkbox(
                    &mut self.save_vtt,
                    egui::RichText::new("Lưu file .vtt gốc").color(TEXT_MAIN),
                );
                if vtt_response.changed() {
                    settings_dirty = true;
                }
                ui.separator();
                ui.label(
                    egui::RichText::new("Mỗi phim lưu trong thư mục con cùng tên")
                        .small()
                        .color(TEXT_FADE),
                );
            });
        });

        if let Some(series) = &self.series {
            let max_num = series.episodes.iter().map(|ep| ep.num).max().unwrap_or(0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("DANH SÁCH TẬP")
                        .small()
                        .strong()
                        .color(TEXT_MAIN),
                );
                ui.separator();
                if ui.button("Chọn tất cả").clicked() {
                    self.selected = series.episodes.iter().map(|ep| ep.num).collect();
                }
                if ui.button("Bỏ chọn").clicked() {
                    self.selected.clear();
                }
                if ui.button("Chọn tập có stream").clicked() {
                    self.selected = series
                        .episodes
                        .iter()
                        .filter(|ep| ep.has_stream && !ep.locked)
                        .map(|ep| ep.num)
                        .collect();
                }
                if ui
                    .button(if self.show_filters {
                        "Ẩn bộ lọc"
                    } else {
                        "Bộ lọc"
                    })
                    .clicked()
                {
                    self.show_filters = !self.show_filters;
                }
                ui.separator();
                ui.label(format!("Đã chọn: {}", self.selected.len()));
                ui.separator();
                let selected_count = self.selected.len();
                let can_download = self.busy == Busy::Idle && selected_count > 0;
                if ui
                    .add_enabled(
                        can_download,
                        egui::Button::new(format!("Tải {selected_count} tập")).fill(BRAND_BUTTON),
                    )
                    .clicked()
                {
                    start_download = true;
                }
                if ui.button("Mở thư mục").clicked() {
                    open_folder = true;
                }
            });

            if self.show_filters {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Lọc:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.ep_filter)
                            .hint_text(egui::RichText::new("tên tập hoặc số tập").color(TEXT_FADE))
                            .desired_width(220.0)
                            .text_color(TEXT_MAIN),
                    );
                    if ui.button("Xóa lọc").clicked() {
                        self.ep_filter.clear();
                    }
                    ui.separator();
                    ui.label("Dải tập:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.range_input)
                            .hint_text(egui::RichText::new("1-10,15").color(TEXT_FADE))
                            .desired_width(130.0)
                            .text_color(TEXT_MAIN),
                    );
                    if ui.button("Áp dụng").clicked() {
                        apply_range = true;
                    }
                    ui.label(format!("tối đa {max_num}"));
                });
            }

            let list_height = ui.available_height().clamp(260.0, 520.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        episode_list_ui(
                            ui,
                            series,
                            &mut self.selected,
                            self.dl.as_ref(),
                            &self.ep_filter,
                        );
                    });
            });
        }

        if self.series.is_none() && self.busy != Busy::Info {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Dán link phim ở trên và bấm \"Tải thông tin\"")
                        .size(18.0)
                        .weak(),
                );
                ui.label(
                    egui::RichText::new(
                        "Hỗ trợ DramaBox, ShortMax, NetShort, FreeReels, KissKH, Hongguo và nhiều nền tảng khác.",
                    )
                    .weak(),
                );
            });
        }

        if start_info {
            self.start_info(ctx);
        }
        if start_download {
            self.start_download(ctx);
        }
        if pick_folder {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Chọn thư mục tải")
                .pick_folder()
            {
                self.output_dir = dir.display().to_string();
                settings_dirty = true;
                self.push_log(format!("Thư mục tải: {}", self.output_dir));
            }
        }
        if open_folder {
            open_output_folder(&self.output_dir);
        }
        if apply_range {
            if let Some(series) = &self.series {
                let valid: HashSet<u32> = series.episodes.iter().map(|ep| ep.num).collect();
                let max_num = valid.iter().copied().max().unwrap_or(0);
                match parse_range(&self.range_input, max_num) {
                    Some(nums) => {
                        self.selected =
                            nums.into_iter().filter(|num| valid.contains(num)).collect();
                        self.push_log(format!(
                            "Áp dụng dải tập \"{}\": {} tập",
                            self.range_input,
                            self.selected.len()
                        ));
                        self.error = None;
                    }
                    None => {
                        self.error =
                            Some("Dải tập không hợp lệ. Ví dụ đúng: 1-10,15,20".to_owned());
                    }
                }
            }
        }
        if settings_dirty {
            self.save_settings();
        }
    }

    fn ui_catalog(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut search = false;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("DUYỆT DANH MỤC")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("catalog_platform")
                    .width(200.0)
                    .selected_text(platform_label(&self.catalog_platform))
                    .show_ui(ui, |ui| {
                        combo_popup_style(ui);
                        for (id, name) in PLATFORMS.iter().skip(1) {
                            ui.selectable_value(
                                &mut self.catalog_platform,
                                (*id).to_owned(),
                                *name,
                            );
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut self.catalog_query)
                        .hint_text(
                            egui::RichText::new("Từ khóa (để trống = mới nhất)").color(TEXT_FADE),
                        )
                        .desired_width(240.0)
                        .text_color(TEXT_MAIN),
                );
                ui.label("Trang");
                ui.add(egui::DragValue::new(&mut self.catalog_page).range(1..=100));
                if ui
                    .add_enabled(
                        self.busy == Busy::Idle,
                        egui::Button::new("Tìm").fill(BRAND_BUTTON),
                    )
                    .clicked()
                {
                    search = true;
                }
                if self.busy == Busy::Catalog {
                    ui.spinner();
                }
            });
            ui.small("Duyệt danh mục đầy đủ hiện hỗ trợ DramaBox và ShortMax.");
        });

        if search {
            self.start_catalog(ctx);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading("Kết quả");
            ui.label(format!(
                "{} mục (trang {})",
                self.catalog_total, self.catalog_page
            ));
        });
        ui.separator();

        let mut chosen: Option<String> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.catalog_items.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Chưa có kết quả. Chọn nền tảng rồi bấm \"Tìm\".")
                                .weak(),
                        );
                    });
                }
                for item in &self.catalog_items {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&item.name).strong().color(TEXT_MAIN));
                                ui.small(format!(
                                    "{} tập  •  ID {}  •  {}",
                                    item.episodes, item.id, item.platform
                                ));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !item.link.is_empty(),
                                            egui::Button::new("Dùng phim này"),
                                        )
                                        .clicked()
                                    {
                                        chosen = Some(item.link.clone());
                                    }
                                },
                            );
                        });
                    });
                    ui.add_space(4.0);
                }
            });

        if let Some(link) = chosen {
            self.url = link;
            self.tab = Tab::Download;
            self.start_info(ctx);
        }
    }

    fn ui_dubbing(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut pick_input = false;
        let mut pick_output = false;
        let mut open_output = false;
        let mut google_pick = false;
        let mut start = false;
        let mut cancel = false;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("LỒNG TIẾNG HÀNG LOẠT")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.small(
                "OCR phụ đề Việt có sẵn → tạo giọng đọc → trộn với âm thanh gốc. \
                 Chỉ dùng cho video bạn có quyền xử lý.",
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Thư mục video:");
                let width = (ui.available_width() - 250.0).max(150.0);
                let folder_before = self.dub_folder.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.dub_folder)
                        .desired_width(width)
                        .hint_text(
                            egui::RichText::new("Chọn thư mục chứa các tập phim...")
                                .color(TEXT_FADE),
                        )
                        .text_color(TEXT_MAIN),
                );
                if response.changed() {
                    let auto = is_auto_dub_output(&self.dub_output, &folder_before);
                    self.dub_video_count = count_videos(&self.dub_folder);
                    if auto {
                        self.dub_output = if self.dub_folder.trim().is_empty() {
                            String::new()
                        } else {
                            default_dub_output(std::path::Path::new(self.dub_folder.trim()))
                                .display()
                                .to_string()
                        };
                    }
                }
                if ui.button("Chọn...").clicked() {
                    pick_input = true;
                }
                if ui.button("Quét lại").clicked() {
                    self.dub_video_count = count_videos(&self.dub_folder);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Thư mục xuất:");
                let width = (ui.available_width() - 250.0).max(150.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.dub_output)
                        .desired_width(width)
                        .hint_text(
                            egui::RichText::new("Mặc định: <thư mục cha>/<tên thư mục> Lồng Tiếng")
                                .color(TEXT_FADE),
                        )
                        .text_color(TEXT_MAIN),
                );
                if ui.button("Chọn...").clicked() {
                    pick_output = true;
                }
                if ui.button("Mở").clicked() {
                    open_output = true;
                }
            });
            if self.dub_video_count > 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 220, 150),
                    format!("Tìm thấy {} video trong thư mục.", self.dub_video_count),
                );
            } else if !self.dub_folder.trim().is_empty() {
                ui.label(
                    egui::RichText::new("Chưa tìm thấy video (mp4/mkv/mov/avi/webm/m4v).")
                        .color(TEXT_FADE),
                );
            }
        });

        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("TUỲ CHỈNH GIỌNG ĐỌC")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.horizontal(|ui| {
                ui.label("Nguồn giọng:");
                let provider_before = self.dub_provider.clone();
                egui::ComboBox::from_id_salt("dub_provider")
                    .width(230.0)
                    .selected_text(tts_provider_label(&self.dub_provider))
                    .show_ui(ui, |ui| {
                        combo_popup_style(ui);
                        for (id, name) in TTS_PROVIDERS {
                            ui.selectable_value(&mut self.dub_provider, (*id).to_owned(), *name);
                        }
                    });
                if self.dub_provider != provider_before {
                    self.dub_voice = default_tts_voice(&self.dub_provider);
                }
                ui.separator();
                ui.label("Giọng:");
                match self.dub_provider.as_str() {
                    "elevenlabs" => {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.dub_voice)
                                .desired_width(230.0)
                                .hint_text(
                                    egui::RichText::new("Voice ID (mặc định: Rachel)")
                                        .color(TEXT_FADE),
                                )
                                .text_color(TEXT_MAIN),
                        );
                    }
                    "gtts" => {
                        ui.label(egui::RichText::new("Tiếng Việt").color(TEXT_MAIN));
                    }
                    _ => {
                        egui::ComboBox::from_id_salt("dub_voice")
                            .width(210.0)
                            .selected_text(dub_voice_display(&self.dub_provider, &self.dub_voice))
                            .show_ui(ui, |ui| {
                                combo_popup_style(ui);
                                match self.dub_provider.as_str() {
                                    "vieneu" => {
                                        for id in VIENEU_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *id,
                                            );
                                        }
                                    }
                                    "zerotts" => {
                                        for (id, name) in ZEROTTS_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *name,
                                            );
                                        }
                                    }
                                    "openai" => {
                                        for id in OPENAI_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *id,
                                            );
                                        }
                                    }
                                    "google" => {
                                        for id in GOOGLE_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *id,
                                            );
                                        }
                                    }
                                    "azure" => {
                                        for id in AZURE_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *id,
                                            );
                                        }
                                    }
                                    _ => {
                                        for (id, name) in DUB_VOICES {
                                            ui.selectable_value(
                                                &mut self.dub_voice,
                                                (*id).to_owned(),
                                                *name,
                                            );
                                        }
                                    }
                                }
                            });
                    }
                }
                ui.separator();
                ui.label("Chế độ OCR:");
                egui::ComboBox::from_id_salt("dub_ocr_mode")
                    .width(250.0)
                    .selected_text(if self.dub_ocr_mode == "fast" {
                        "Nhanh — RapidOCR (thiếu dấu)"
                    } else {
                        "Chất lượng — VietOCR (khuyến nghị)"
                    })
                    .show_ui(ui, |ui| {
                        combo_popup_style(ui);
                        ui.selectable_value(
                            &mut self.dub_ocr_mode,
                            "quality".to_owned(),
                            "Chất lượng — VietOCR (khuyến nghị)",
                        );
                        ui.selectable_value(
                            &mut self.dub_ocr_mode,
                            "fast".to_owned(),
                            "Nhanh — RapidOCR (thiếu dấu)",
                        );
                    });
            });
            match self.dub_provider.as_str() {
                "elevenlabs" => {
                    ui.horizontal(|ui| {
                        ui.label("ElevenLabs API key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tts_api_key)
                                .password(true)
                                .desired_width(360.0)
                                .hint_text(
                                    egui::RichText::new("Dán API key từ elevenlabs.io")
                                        .color(TEXT_FADE),
                                )
                                .text_color(TEXT_MAIN),
                        );
                    });
                }
                "openai" => {
                    ui.horizontal(|ui| {
                        ui.label("OpenAI API key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tts_api_key)
                                .password(true)
                                .desired_width(360.0)
                                .hint_text(
                                    egui::RichText::new("Dán API key từ platform.openai.com")
                                        .color(TEXT_FADE),
                                )
                                .text_color(TEXT_MAIN),
                        );
                    });
                }
                "google" => {
                    ui.horizontal(|ui| {
                        ui.label("Credentials JSON:");
                        let width = (ui.available_width() - 130.0).max(150.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tts_credentials)
                                .desired_width(width)
                                .hint_text(
                                    egui::RichText::new(
                                        "Đường dẫn file service-account JSON của Google Cloud",
                                    )
                                    .color(TEXT_FADE),
                                )
                                .text_color(TEXT_MAIN),
                        );
                        if ui.button("Chọn...").clicked() {
                            google_pick = true;
                        }
                    });
                }
                "azure" => {
                    ui.horizontal(|ui| {
                        ui.label("Azure key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tts_api_key)
                                .password(true)
                                .desired_width(280.0)
                                .hint_text(egui::RichText::new("Speech key").color(TEXT_FADE))
                                .text_color(TEXT_MAIN),
                        );
                        ui.label("Region:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tts_region)
                                .desired_width(110.0)
                                .hint_text(egui::RichText::new("southeastasia").color(TEXT_FADE))
                                .text_color(TEXT_MAIN),
                        );
                    });
                }
                "zerotts" => {
                    ui.small(
                        "ZeroTTS local (MIT): lần đầu tải model ~900 MB, sau đó chạy offline trên CPU. \
                         Cài bằng: pip install zerotts",
                    );
                }
                "gtts" => {
                    ui.small(
                        "gTTS dùng Google Translate TTS (miễn phí), cần mạng và có thể bị giới hạn tần suất.",
                    );
                }
                _ => {}
            }
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut self.dub_original_enabled,
                    egui::RichText::new("Phát âm thanh gốc của video").color(TEXT_MAIN),
                );
                ui.add_enabled(
                    self.dub_original_enabled,
                    egui::Slider::new(&mut self.dub_original_volume, 0..=100)
                        .text("Âm gốc (%)"),
                );
                ui.add(
                    egui::Slider::new(&mut self.dub_tts_volume, 0..=200)
                        .text("Âm lồng tiếng (%)"),
                );
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut self.dub_rate, -30..=30).text("Tốc độ đọc (%)"),
                );
                ui.checkbox(
                    &mut self.dub_use_sidecar,
                    egui::RichText::new("Ưu tiên SRT/VTT có sẵn").color(TEXT_MAIN),
                );
                ui.checkbox(
                    &mut self.dub_overwrite,
                    egui::RichText::new("Ghi đè file đã có").color(TEXT_MAIN),
                );
                ui.checkbox(
                    &mut self.dub_ocr_only,
                    egui::RichText::new("Chỉ tạo SRT (không lồng tiếng)").color(TEXT_MAIN),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(
                    &mut self.dub_soft_timing,
                    egui::RichText::new("Soft Timing (dời câu vào khoảng lặng)")
                        .color(TEXT_MAIN),
                )
                .on_hover_text("Dời giọng đọc sớm tối đa 0,25 giây vào khoảng lặng giữa hai câu.");
                ui.checkbox(
                    &mut self.dub_natural_voice,
                    egui::RichText::new("Voice tự nhiên (chồng nhẹ đuôi câu)").color(TEXT_MAIN),
                )
                .on_hover_text("Cho phép đuôi câu dài hơn phụ đề tối đa 15% để giọng tự nhiên hơn.");
                ui.checkbox(
                    &mut self.dub_keep_temp,
                    egui::RichText::new("Giữ file tạm (SRT + cache TTS)").color(TEXT_MAIN),
                )
                .on_hover_text(
                    "Mặc định app tự xóa _phude và _tts_cache sau khi xong, chỉ giữ video lồng tiếng. \
                     Bật tuỳ chọn này nếu muốn giữ SRT để sửa rồi chạy lại.",
                );
            });
            ui.small(
                "Video lồng tiếng được lưu trong thư mục \"<tên thư mục> Lồng Tiếng\". Mặc định app tự xóa \
                 _phude và _tts_cache sau khi xong; bật \"Giữ file tạm\" nếu muốn sửa SRT rồi chạy lại. \
                 Các thiết lập và mức âm lượng được tự lưu khi thoát app.",
            );
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_start = self.busy == Busy::Idle && self.dub_video_count > 0;
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new("Bắt đầu lồng tiếng").fill(BRAND_BUTTON),
                )
                .clicked()
            {
                start = true;
            }
            if self.busy == Busy::Dubbing {
                if ui
                    .add(egui::Button::new("Hủy").fill(BRAND_BUTTON_HOVER))
                    .clicked()
                {
                    cancel = true;
                }
            }
            if ui.button("Mở thư mục xuất").clicked() {
                open_output = true;
            }
            if self.busy == Busy::Dubbing {
                ui.spinner();
            }
        });

        if self.dub_total > 0 {
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(
                    egui::RichText::new("TIẾN ĐỘ")
                        .small()
                        .strong()
                        .color(TEXT_MAIN),
                );
                ui.add(
                    egui::ProgressBar::new(self.dub_progress.clamp(0.0, 1.0))
                        .desired_width(ui.available_width())
                        .show_percentage(),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{} / {} video",
                        self.dub_done + self.dub_failed,
                        self.dub_total
                    ));
                    if self.dub_failed > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 120, 120),
                            format!("{} lỗi", self.dub_failed),
                        );
                    }
                    if !self.dub_current.is_empty() {
                        ui.separator();
                        ui.label(&self.dub_current);
                    }
                });
                if !self.dub_stage.is_empty() {
                    ui.label(egui::RichText::new(&self.dub_stage).color(TEXT_FADE));
                }
                if self.dub_finished {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 150),
                        "Hoàn tất. Video đã lồng tiếng nằm trong thư mục xuất.",
                    );
                }
            });
        }

        if google_pick {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Chọn file service-account JSON của Google Cloud")
                .add_filter("JSON", &["json"])
                .pick_file()
            {
                self.tts_credentials = path.display().to_string();
            }
        }
        if pick_input {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Chọn thư mục chứa video")
                .pick_folder()
            {
                let auto = is_auto_dub_output(&self.dub_output, &self.dub_folder);
                self.dub_folder = dir.display().to_string();
                self.dub_video_count = count_videos(&self.dub_folder);
                if auto {
                    self.dub_output = default_dub_output(&dir).display().to_string();
                }
            }
        }
        if pick_output {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Chọn thư mục xuất")
                .pick_folder()
            {
                self.dub_output = dir.display().to_string();
            }
        }
        if open_output {
            let target = if self.dub_output.trim().is_empty() {
                if self.dub_folder.trim().is_empty() {
                    self.output_dir.clone()
                } else {
                    default_dub_output(std::path::Path::new(self.dub_folder.trim()))
                        .display()
                        .to_string()
                }
            } else {
                self.dub_output.trim().to_owned()
            };
            open_output_folder(&target);
        }
        if cancel {
            self.cancel();
        }
        if start {
            self.start_dubbing(ctx);
        }
    }

    fn ui_merge(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut pick_input = false;
        let mut pick_output = false;
        let mut open_output = false;
        let mut start = false;
        let mut cancel = false;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("GHÉP FULL TẬP")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.small(
                "Nối toàn bộ video trong thư mục thành 1 file duy nhất. Dùng copy trực tiếp \
                 (-c copy) nên rất nhanh, không render lại.",
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Thư mục video:");
                let width = (ui.available_width() - 250.0).max(150.0);
                let folder_before = self.merge_folder.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.merge_folder)
                        .desired_width(width)
                        .hint_text(
                            egui::RichText::new("Chọn thư mục chứa các tập phim...")
                                .color(TEXT_FADE),
                        )
                        .text_color(TEXT_MAIN),
                );
                if response.changed() {
                    let auto = is_auto_merge_output(&self.merge_output, &folder_before);
                    self.merge_video_count = count_videos(&self.merge_folder);
                    if auto {
                        self.merge_output = if self.merge_folder.trim().is_empty() {
                            String::new()
                        } else {
                            default_merge_output(std::path::Path::new(self.merge_folder.trim()))
                                .display()
                                .to_string()
                        };
                    }
                }
                if ui.button("Chọn...").clicked() {
                    pick_input = true;
                }
                if ui.button("Quét lại").clicked() {
                    self.merge_video_count = count_videos(&self.merge_folder);
                }
            });
            ui.horizontal(|ui| {
                ui.label("File đầu ra:");
                let width = (ui.available_width() - 250.0).max(150.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.merge_output)
                        .desired_width(width)
                        .hint_text(
                            egui::RichText::new("Mặc định: <thư mục cha>/<tên thư mục> Full.mp4")
                                .color(TEXT_FADE),
                        )
                        .text_color(TEXT_MAIN),
                );
                if ui.button("Chọn...").clicked() {
                    pick_output = true;
                }
                if ui.button("Mở").clicked() {
                    open_output = true;
                }
            });
            if self.merge_video_count > 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 220, 150),
                    format!(
                        "Tìm thấy {} video. Sẽ ghép theo thứ tự EP01, EP02, ...",
                        self.merge_video_count
                    ),
                );
            } else if !self.merge_folder.trim().is_empty() {
                ui.label(
                    egui::RichText::new("Chưa tìm thấy video (mp4/mkv/mov/avi/webm/m4v).")
                        .color(TEXT_FADE),
                );
            }
        });

        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                egui::RichText::new("TUỲ CHỌN")
                    .small()
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(
                    &mut self.merge_overwrite,
                    egui::RichText::new("Ghi đè file đã có").color(TEXT_MAIN),
                );
                ui.checkbox(
                    &mut self.merge_reencode,
                    egui::RichText::new("Ghép lại nếu lỗi (re-encode, chậm hơn)").color(TEXT_MAIN),
                )
                .on_hover_text(
                    "Mặc định nối trực tiếp bằng -c copy. Nếu các tập khác codec/độ phân giải, \
                     bật tuỳ chọn này để ghép lại.",
                );
            });
            ui.small(
                "Các tập tải cùng nền tảng thường cùng định dạng nên ghép copy chỉ mất vài giây.",
            );
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_start = self.busy == Busy::Idle && self.merge_video_count > 0;
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new("Bắt đầu ghép").fill(BRAND_BUTTON),
                )
                .clicked()
            {
                start = true;
            }
            if self.busy == Busy::Merging {
                if ui
                    .add(egui::Button::new("Hủy").fill(BRAND_BUTTON_HOVER))
                    .clicked()
                {
                    cancel = true;
                }
            }
            if ui.button("Mở thư mục xuất").clicked() {
                open_output = true;
            }
            if self.busy == Busy::Merging {
                ui.spinner();
            }
        });

        if self.merge_total > 0 {
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(
                    egui::RichText::new("TIẾN ĐỘ")
                        .small()
                        .strong()
                        .color(TEXT_MAIN),
                );
                ui.add(
                    egui::ProgressBar::new(self.merge_progress.clamp(0.0, 1.0))
                        .desired_width(ui.available_width())
                        .show_percentage(),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} video", self.merge_total));
                    if !self.merge_stage.is_empty() {
                        ui.separator();
                        ui.label(&self.merge_stage);
                    }
                });
                if !self.merge_output_path.is_empty() {
                    ui.label(egui::RichText::new(&self.merge_output_path).color(TEXT_FADE));
                }
                if self.merge_finished {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 150),
                        "Hoàn tất. File full nằm trong thư mục xuất.",
                    );
                }
            });
        }

        if pick_input {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Chọn thư mục chứa các tập phim")
                .pick_folder()
            {
                let auto = is_auto_merge_output(&self.merge_output, &self.merge_folder);
                self.merge_folder = dir.display().to_string();
                self.merge_video_count = count_videos(&self.merge_folder);
                if auto {
                    self.merge_output = default_merge_output(&dir).display().to_string();
                }
            }
        }
        if pick_output {
            let suggested = if self.merge_folder.trim().is_empty() {
                "Full.mp4".to_owned()
            } else {
                default_merge_output(std::path::Path::new(self.merge_folder.trim()))
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Full.mp4")
                    .to_owned()
            };
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Chọn file đầu ra")
                .set_file_name(&suggested)
                .add_filter("Video MP4", &["mp4"])
                .save_file()
            {
                self.merge_output = path.display().to_string();
            }
        }
        if open_output {
            let target = if self.merge_output.trim().is_empty() {
                if self.merge_folder.trim().is_empty() {
                    self.output_dir.clone()
                } else {
                    default_merge_output(std::path::Path::new(self.merge_folder.trim()))
                        .display()
                        .to_string()
                }
            } else {
                self.merge_output.trim().to_owned()
            };
            open_output_folder(&target);
        }
        if cancel {
            self.cancel();
        }
        if start {
            self.start_merge(ctx);
        }
    }

    fn ui_donate(&mut self, ui: &mut egui::Ui) {
        let mut copied: Option<&'static str> = None;

        if self.donate_texture.is_none() {
            self.donate_texture = load_texture(
                ui.ctx(),
                "donate-qr",
                include_bytes!("../assets/donate_qr_only.png"),
            );
        }

        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("ỦNG HỘ DRAMAAI")
                    .small()
                    .strong()
                    .color(BRAND_ORANGE),
            );
        });
        ui.add_space(4.0);

        ui.vertical_centered(|ui| {
            if let Some(texture) = &self.donate_texture {
                let source = texture.size_vec2();
                let width = 200.0;
                let height = if source.x > 0.0 {
                    width * source.y / source.x
                } else {
                    290.0
                };
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    texture.id(),
                    egui::vec2(width, height),
                )));
            } else {
                ui.label("Không tải được ảnh QR.");
            }
        });

        ui.add_space(4.0);

        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("VIETCOMBANK")
                    .size(20.0)
                    .strong()
                    .color(BRAND_PINK),
            );
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(
                egui::RichText::new("Số tài khoản: 1016581189")
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.label(
                egui::RichText::new("Chủ tài khoản: LE THANH THAI DUONG")
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.label(
                egui::RichText::new("Nội dung: DramaAI donate")
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(8.0);
            if ui
                .add(egui::Button::new("Sao chép số tài khoản").fill(BRAND_BUTTON))
                .clicked()
            {
                ui.ctx().copy_text("1016581189".to_owned());
                copied = Some("Đã sao chép số tài khoản");
            }
        });

        if let Some(message) = copied {
            self.push_log(message);
        }
    }

    fn ui_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Nhật ký");
            if ui.button("Xóa nhật ký").clicked() {
                self.log.clear();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            gradient_bar(ui, 5.0);
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("DramaAI").color(BRAND_PINK));
                ui.heading("Tiktok Downloader");
                ui.label(
                    egui::RichText::new("Rust GUI • Python core • v1.3.1")
                        .small()
                        .weak(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} nền tảng", PLATFORMS.len() - 1))
                            .small()
                            .weak(),
                    );
                });
            });
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                for tab in [
                    Tab::Download,
                    Tab::Catalog,
                    Tab::Dubbing,
                    Tab::Merge,
                    Tab::Donate,
                    Tab::Log,
                ] {
                    if tab_button(ui, self.tab == tab, tab.label()) {
                        self.tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            gradient_bar(ui, 2.0);
        });

        let mut cancel_download = false;
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy != Busy::Idle {
                    ui.spinner();
                }
                ui.label(&self.status);
                if let Some(error) = &self.error {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(255, 120, 120), error);
                }
                if let Some(dl) = &self.dl {
                    let partial: f32 = dl
                        .ep_progress
                        .values()
                        .filter(|progress| **progress < 1.0)
                        .sum();
                    let fraction = if dl.total > 0 {
                        ((dl.done + dl.failed) as f32 + partial) / dl.total as f32
                    } else {
                        0.0
                    };
                    ui.separator();
                    ui.add(
                        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                            .desired_width(150.0)
                            .show_percentage(),
                    );
                    if self.busy == Busy::Download && !dl.finished {
                        if ui.small_button("Hủy").clicked() {
                            cancel_download = true;
                        }
                    }
                    if dl.failed > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 120, 120),
                            format!("{} lỗi", dl.failed),
                        );
                    }
                }
                if self.busy == Busy::Dubbing || self.dub_total > 0 {
                    ui.separator();
                    ui.add(
                        egui::ProgressBar::new(self.dub_progress.clamp(0.0, 1.0))
                            .desired_width(150.0)
                            .show_percentage(),
                    );
                    if self.busy == Busy::Dubbing {
                        if ui.small_button("Hủy").clicked() {
                            cancel_download = true;
                        }
                    }
                    if self.dub_failed > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 120, 120),
                            format!("{} lỗi lồng tiếng", self.dub_failed),
                        );
                    }
                }
                if self.busy == Busy::Merging || self.merge_total > 0 {
                    ui.separator();
                    ui.add(
                        egui::ProgressBar::new(self.merge_progress.clamp(0.0, 1.0))
                            .desired_width(150.0)
                            .show_percentage(),
                    );
                    if self.busy == Busy::Merging {
                        if ui.small_button("Hủy").clicked() {
                            cancel_download = true;
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(series) = &self.series {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} • {} tập",
                                series.platform.to_uppercase(),
                                series.episode_count
                            ))
                            .small()
                            .weak(),
                        );
                    }
                });
            });
        });
        if cancel_download {
            self.cancel();
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Download => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.ui_download(ui, ctx));
            }
            Tab::Catalog => self.ui_catalog(ui, ctx),
            Tab::Dubbing => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.ui_dubbing(ui, ctx));
            }
            Tab::Merge => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.ui_merge(ui, ctx));
            }
            Tab::Donate => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.ui_donate(ui));
            }
            Tab::Log => self.ui_log(ui),
        });

        if self.busy != Busy::Idle {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.save_settings();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn episode_list_ui(
    ui: &mut egui::Ui,
    series: &SeriesInfo,
    selected: &mut HashSet<u32>,
    dl: Option<&DownloadState>,
    filter: &str,
) {
    let needle = filter.trim().to_lowercase();
    let mut visible = 0usize;

    for ep in &series.episodes {
        if !needle.is_empty() {
            let haystack = format!("{} {}", ep.num, ep.title).to_lowercase();
            if !haystack.contains(&needle) {
                continue;
            }
        }
        visible += 1;

        ui.horizontal(|ui| {
            let mut checked = selected.contains(&ep.num);
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    selected.insert(ep.num);
                } else {
                    selected.remove(&ep.num);
                }
            }
            ui.monospace(format!("EP{:03}", ep.num));

            if ep.locked {
                ui.colored_label(egui::Color32::from_rgb(235, 160, 90), "KHÓA");
            } else if ep.has_stream {
                ui.colored_label(egui::Color32::from_rgb(120, 220, 150), "FREE");
            } else {
                ui.colored_label(egui::Color32::from_rgb(150, 150, 160), "NO STREAM");
            }

            if ep.subs > 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 180, 255),
                    format!("{} phụ đề", ep.subs),
                );
            }

            ui.label(&ep.title);

            if let Some(dl) = dl {
                if let Some(progress) = dl.ep_progress.get(&ep.num) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if *progress >= 1.0 {
                            ui.colored_label(egui::Color32::from_rgb(120, 220, 150), "xong");
                        } else if *progress > 0.0 {
                            ui.add(
                                egui::ProgressBar::new(*progress)
                                    .desired_width(110.0)
                                    .show_percentage(),
                            );
                        }
                    });
                }
            }
        });
    }

    if visible == 0 {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Không có tập nào khớp bộ lọc.").weak());
    }
}

fn combo_popup_style(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    // Popup rows sit on the dark window background, so unselected text must be
    // light. Hovered/selected rows use pastel fills and keep dark text.
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT_MAIN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, BUTTON_TEXT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, BUTTON_TEXT);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.5_f32, BUTTON_TEXT);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, BUTTON_TEXT);
}

fn tab_button(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let text = egui::RichText::new(label).color(if selected { BUTTON_TEXT } else { TEXT_MAIN });
    let button = egui::Button::new(text)
        .frame(selected)
        .fill(if selected {
            BRAND_BUTTON
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(if selected {
            egui::Stroke::new(1.0_f32, BRAND_PINK)
        } else {
            egui::Stroke::NONE
        })
        .min_size(egui::vec2(0.0, 30.0))
        .rounding(egui::Rounding::same(8.0));
    ui.add(button).clicked()
}

fn gradient_bar(ui: &mut egui::Ui, height: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), BRAND_PINK);
    mesh.colored_vertex(rect.right_top(), BRAND_ORANGE);
    mesh.colored_vertex(rect.left_bottom(), BRAND_PINK);
    mesh.colored_vertex(rect.right_bottom(), BRAND_ORANGE);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn load_window_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()?
        .to_rgba8();
    Some(egui::IconData {
        rgba: image.as_raw().clone(),
        width: image.width(),
        height: image.height(),
    })
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "latin-ui".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "latin-ui".to_owned());
        }
    }

    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
        fonts.font_data.insert(
            "cjk-ui".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("cjk-ui".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.push("cjk-ui".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 26.0;
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(21.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
    ctx.set_style(style);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(15, 10, 15);
    visuals.window_fill = egui::Color32::from_rgb(23, 14, 22);
    visuals.extreme_bg_color = egui::Color32::from_rgb(9, 6, 10);
    visuals.faint_bg_color = egui::Color32::from_rgb(31, 20, 29);
    visuals.selection.bg_fill = BRAND_BUTTON;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, BRAND_PINK);
    visuals.hyperlink_color = LINK_COLOR;

    let rounding = egui::Rounding::same(8.0);
    visuals.widgets.noninteractive.rounding = rounding;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT_MAIN);
    visuals.widgets.noninteractive.weak_bg_fill = TEXT_FADE;
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(92, 62, 82));

    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.bg_fill = BRAND_BUTTON;
    visuals.widgets.inactive.weak_bg_fill = BRAND_BUTTON;
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(230, 135, 163));
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, BUTTON_TEXT);

    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_fill = BRAND_BUTTON_HOVER;
    visuals.widgets.hovered.weak_bg_fill = BRAND_BUTTON_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, BRAND_PINK);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, BUTTON_TEXT);

    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_fill = BRAND_BUTTON_ACTIVE;
    visuals.widgets.active.weak_bg_fill = BRAND_BUTTON_ACTIVE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, BRAND_PINK);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, BUTTON_TEXT);

    visuals.widgets.open.rounding = rounding;
    visuals.widgets.open.bg_fill = BRAND_BUTTON_HOVER;
    visuals.widgets.open.weak_bg_fill = BRAND_BUTTON_HOVER;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, BRAND_PINK);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, BUTTON_TEXT);

    ctx.set_visuals(visuals);
}

fn platform_label(id: &str) -> &'static str {
    PLATFORMS
        .iter()
        .find(|(platform_id, _)| *platform_id == id)
        .map(|(_, name)| *name)
        .unwrap_or("Tự động nhận diện")
}

fn dub_voice_label(id: &str) -> &'static str {
    DUB_VOICES
        .iter()
        .find(|(voice_id, _)| *voice_id == id)
        .map(|(_, name)| *name)
        .unwrap_or("Giọng mặc định")
}

fn tts_provider_label(id: &str) -> &'static str {
    TTS_PROVIDERS
        .iter()
        .find(|(provider_id, _)| *provider_id == id)
        .map(|(_, name)| *name)
        .unwrap_or("Edge TTS")
}

fn default_tts_voice(provider: &str) -> String {
    match provider {
        "edge" => DUB_VOICES[0].0.to_owned(),
        "vieneu" => VIENEU_VOICES[0].to_owned(),
        "zerotts" => ZEROTTS_VOICES[0].0.to_owned(),
        "gtts" => "vi".to_owned(),
        "elevenlabs" => ELEVENLABS_DEFAULT_VOICE.to_owned(),
        "openai" => OPENAI_VOICES[0].to_owned(),
        "google" => GOOGLE_VOICES[0].to_owned(),
        "azure" => AZURE_VOICES[0].to_owned(),
        _ => DUB_VOICES[0].0.to_owned(),
    }
}

fn is_voice_valid(provider: &str, voice: &str) -> bool {
    match provider {
        "edge" => DUB_VOICES.iter().any(|(id, _)| *id == voice),
        "vieneu" => VIENEU_VOICES.contains(&voice),
        "zerotts" => ZEROTTS_VOICES.iter().any(|(id, _)| *id == voice),
        "gtts" => voice == "vi",
        "openai" => OPENAI_VOICES.contains(&voice),
        "google" => GOOGLE_VOICES.contains(&voice),
        "azure" => AZURE_VOICES.contains(&voice),
        "elevenlabs" => !voice.trim().is_empty(),
        _ => false,
    }
}

fn dub_voice_display<'a>(provider: &str, voice: &'a str) -> &'a str {
    match provider {
        "edge" => dub_voice_label(voice),
        "zerotts" => ZEROTTS_VOICES
            .iter()
            .find(|(id, _)| *id == voice)
            .map(|(_, name)| *name)
            .unwrap_or(voice),
        _ => voice,
    }
}

fn default_dub_output(folder: &std::path::Path) -> PathBuf {
    let name = folder
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    let parent = folder
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(folder);
    parent.join(format!("{name} Lồng Tiếng"))
}

fn default_merge_output(folder: &std::path::Path) -> PathBuf {
    let name = folder
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    let parent = folder
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(folder);
    parent.join(format!("{name} Full.mp4"))
}

fn is_auto_dub_output(output: &str, folder: &str) -> bool {
    let output = output.trim();
    if output.is_empty() {
        return true;
    }
    if folder.trim().is_empty() {
        return false;
    }
    let default = default_dub_output(std::path::Path::new(folder.trim()))
        .display()
        .to_string();
    output.eq_ignore_ascii_case(&default)
}

fn is_auto_merge_output(output: &str, folder: &str) -> bool {
    let output = output.trim();
    if output.is_empty() {
        return true;
    }
    if folder.trim().is_empty() {
        return false;
    }
    let default = default_merge_output(std::path::Path::new(folder.trim()))
        .display()
        .to_string();
    output.eq_ignore_ascii_case(&default)
}

fn count_videos(folder: &str) -> usize {
    const EXTENSIONS: &[&str] = &["mp4", "mkv", "mov", "avi", "webm", "m4v"];
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .count()
}

fn parse_range(input: &str, max_episode: u32) -> Option<Vec<u32>> {
    let mut result = HashSet::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start_raw, end_raw)) = part.split_once('-') {
            let start: u32 = start_raw.trim().parse().ok()?;
            let end: u32 = if end_raw.trim().is_empty() {
                max_episode
            } else {
                end_raw.trim().parse().ok()?
            };
            if start == 0 || end < start {
                return None;
            }
            for number in start..=end.min(max_episode) {
                result.insert(number);
            }
        } else {
            let number: u32 = part.parse().ok()?;
            if number == 0 {
                return None;
            }
            result.insert(number);
        }
    }
    if result.is_empty() {
        return None;
    }
    let mut numbers: Vec<u32> = result.into_iter().collect();
    numbers.sort_unstable();
    Some(numbers)
}

fn python_exe() -> String {
    std::env::var("DRAMA_PYTHON").unwrap_or_else(|_| "python".to_owned())
}

fn project_root() -> PathBuf {
    if let Ok(root) = std::env::var("DRAMA_ROOT") {
        return PathBuf::from(root);
    }

    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            if ancestor
                .join("drama_downloader")
                .join("bridge.py")
                .is_file()
            {
                return ancestor.to_path_buf();
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn open_output_folder(directory: &str) {
    let path = PathBuf::from(directory);
    let path = if path.is_absolute() {
        path
    } else {
        project_root().join(path)
    };

    let _ = std::fs::create_dir_all(&path);

    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(&path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(&path).spawn();
    }
}

fn now_hms() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (seconds / 3600) % 24;
    let minutes = (seconds / 60) % 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

fn run_smoke(target: &str) -> bool {
    let ctx = egui::Context::default();
    let mut app = App::base();
    app.url = target.to_owned();
    app.start_info(&ctx);

    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        app.poll_events();
        if app.busy == Busy::Idle && (app.series.is_some() || app.error.is_some()) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    if let Some(series) = &app.series {
        println!(
            "SMOKE_OK platform={} episodes={} selected={}",
            series.platform,
            series.episodes.len(),
            app.selected.len()
        );
        true
    } else {
        println!(
            "SMOKE_FAIL error={}",
            app.error.as_deref().unwrap_or("timeout").escape_debug()
        );
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_assets_decode() {
        let qr = image::load_from_memory(include_bytes!("../assets/donate_vietqr.png"))
            .expect("VietQR PNG must decode");
        assert!(qr.width() >= 300 && qr.height() >= 300);

        let qr_only = image::load_from_memory(include_bytes!("../assets/donate_qr_only.png"))
            .expect("QR-only PNG must decode");
        assert_eq!((qr_only.width(), qr_only.height()), (409, 409));

        let icon = image::load_from_memory(include_bytes!("../assets/icon.png"))
            .expect("icon PNG must decode");
        assert_eq!((icon.width(), icon.height()), (256, 256));
    }

    #[test]
    fn episode_range_parser() {
        assert_eq!(
            super::parse_range("1-5,10,15-", 20),
            Some(vec![1, 2, 3, 4, 5, 10, 15, 16, 17, 18, 19, 20])
        );
        assert_eq!(super::parse_range("abc", 20), None);
    }

    #[test]
    fn video_counter_filters_extensions() {
        let folder = std::env::temp_dir().join(format!("dramaai-count-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("create temp folder");
        std::fs::write(folder.join("a.mp4"), b"").expect("write mp4");
        std::fs::write(folder.join("b.MKV"), b"").expect("write mkv");
        std::fs::write(folder.join("c.txt"), b"").expect("write txt");
        std::fs::create_dir_all(folder.join("d.mp4")).expect("create dir");
        assert_eq!(super::count_videos(folder.to_str().expect("utf-8 path")), 2);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn auto_output_detection() {
        assert!(super::is_auto_dub_output("", "C:\\Phim AI\\Phim"));
        assert!(super::is_auto_dub_output(
            "C:\\Phim AI\\Phim Lồng Tiếng",
            "C:\\Phim AI\\Phim"
        ));
        assert!(!super::is_auto_dub_output(
            "D:\\Custom\\out",
            "C:\\Phim AI\\Phim"
        ));
        assert!(super::is_auto_merge_output("", "C:\\Phim AI\\Phim"));
        assert!(super::is_auto_merge_output(
            "C:\\Phim AI\\Phim Full.mp4",
            "C:\\Phim AI\\Phim"
        ));
        assert!(!super::is_auto_merge_output(
            "D:\\Custom\\Full.mp4",
            "C:\\Phim AI\\Phim"
        ));
        assert!(super::is_auto_merge_output(
            "C:\\PHIM AI\\PHIM FULL.MP4",
            "C:\\Phim AI\\Phim"
        ));
    }

    #[test]
    fn settings_roundtrip_keeps_dubbing_values() {
        let old = r#"{"output_dir":"C:\\Phim AI","concurrency":2,"save_vtt":true,"platform":"dramabox","last_url":"dramabox:1"}"#;
        let mut settings: super::Settings = serde_json::from_str(old).expect("old settings");
        assert_eq!(settings.dub_provider, "edge");
        assert_eq!(settings.dub_voice, super::DUB_VOICES[0].0);
        assert!(settings.dub_original_enabled);
        assert_eq!(settings.dub_original_volume, 15);
        assert_eq!(settings.dub_tts_volume, 100);

        settings.catalog_platform = "shortmax".to_owned();
        settings.dub_folder = "C:\\Phim AI\\Phim".to_owned();
        settings.dub_output = "C:\\Phim AI\\LongTieng".to_owned();
        settings.dub_provider = "vieneu".to_owned();
        settings.dub_voice = "Hải Đăng".to_owned();
        settings.dub_original_enabled = false;
        settings.dub_original_volume = 50;
        settings.dub_tts_volume = 140;
        settings.dub_rate = -5;
        settings.dub_use_sidecar = false;
        settings.dub_overwrite = true;
        settings.dub_ocr_only = true;
        settings.dub_ocr_mode = "fast".to_owned();
        settings.dub_soft_timing = true;
        settings.dub_natural_voice = false;
        settings.dub_keep_temp = true;
        settings.merge_folder = "C:\\Phim AI\\Phim".to_owned();
        settings.merge_output = "C:\\Phim AI\\Phim Full.mp4".to_owned();
        settings.merge_overwrite = false;
        settings.merge_reencode = true;
        settings.tts_api_key = "key-123".to_owned();
        settings.tts_region = "southeastasia".to_owned();
        settings.tts_credentials = "C:\\keys\\google.json".to_owned();

        let text = serde_json::to_string(&settings).expect("serialize settings");
        let roundtrip: super::Settings = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(roundtrip.catalog_platform, "shortmax");
        assert_eq!(roundtrip.dub_provider, "vieneu");
        assert_eq!(roundtrip.dub_voice, "Hải Đăng");
        assert!(!roundtrip.dub_original_enabled);
        assert_eq!(roundtrip.dub_original_volume, 50);
        assert_eq!(roundtrip.dub_tts_volume, 140);
        assert_eq!(roundtrip.dub_rate, -5);
        assert!(!roundtrip.dub_use_sidecar);
        assert!(roundtrip.dub_overwrite);
        assert!(roundtrip.dub_ocr_only);
        assert_eq!(roundtrip.dub_ocr_mode, "fast");
        assert!(roundtrip.dub_soft_timing);
        assert!(!roundtrip.dub_natural_voice);
        assert!(roundtrip.dub_keep_temp);
        assert_eq!(roundtrip.merge_folder, "C:\\Phim AI\\Phim");
        assert_eq!(roundtrip.merge_output, "C:\\Phim AI\\Phim Full.mp4");
        assert!(!roundtrip.merge_overwrite);
        assert!(roundtrip.merge_reencode);
        assert_eq!(roundtrip.tts_api_key, "key-123");
        assert_eq!(roundtrip.tts_region, "southeastasia");
        assert_eq!(roundtrip.tts_credentials, "C:\\keys\\google.json");
    }
}

fn main() -> eframe::Result {
    if let Ok(target) = std::env::var("DRAMA_GUI_SMOKE") {
        let ok = run_smoke(&target);
        std::process::exit(if ok { 0 } else { 1 });
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 680.0])
        .with_min_inner_size([820.0, 560.0])
        .with_resizable(true)
        .with_title("DramaAI Tiktok Downloader v1.3.1");
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "DramaAI Tiktok Downloader v1.3.1",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
