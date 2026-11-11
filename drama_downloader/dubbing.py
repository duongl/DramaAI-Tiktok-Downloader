"""
drama_downloader.dubbing
~~~~~~~~~~~~~~~~~~~~~~~~
Batch hardsub OCR + Vietnamese TTS dubbing.

Pipeline for each video:
1. Read a band around the burned-in subtitle with ffmpeg (1 fps).
2. Detect text lines with RapidOCR; read Vietnamese accents with VietOCR
   (fast mode uses RapidOCR's Latin model instead).
3. Build a .srt (saved for review/reuse).
4. Synthesize Vietnamese speech with edge-tts (online) or VieNeu-TTS
   (optional local 48 kHz engine with 25 preset voices).
5. Mix the TTS track with the original audio using ffmpeg.

Only use this on content you own or have permission to process.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import unicodedata
from dataclasses import dataclass
from difflib import SequenceMatcher
from pathlib import Path
from typing import Any, Callable, Iterable, Optional

from .downloader import vtt_to_srt

VIDEO_EXTENSIONS = {".mp4", ".mkv", ".mov", ".avi", ".webm", ".m4v"}
SAMPLE_FPS = 1.0
SUBTITLE_TOP = 0.55
SUBTITLE_BOTTOM = 0.92

VI_VOICES = {
    "vi-VN-HoaiMyNeural": "Nữ — Hoài My",
    "vi-VN-NamMinhNeural": "Nam — Nam Minh",
}

TTS_PROVIDERS = {
    "edge": "Edge TTS (online)",
    "vieneu": "VieNeu-TTS (local, 48kHz)",
}

# 25 preset voices của VieNeu-TTS v3 Turbo (Apache-2.0).
VIENEU_VOICES = [
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
]

_EASY_READER = None
_RAPID_ENGINE = None
_RAPID_ENGINE_LATIN = None
_VIETOCR_PREDICTOR = None
_VIENEU_ENGINE = None
_BASE_WORDS: Optional[dict[str, str]] = None


def _which(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise RuntimeError(f"Không tìm thấy {name} trong PATH.")
    return path


def list_videos(folder: str | Path) -> list[Path]:
    root = Path(folder)
    if not root.is_dir():
        return []
    return sorted(
        path
        for path in root.iterdir()
        if path.is_file() and path.suffix.lower() in VIDEO_EXTENSIONS
    )


def default_output_dir(folder: str | Path) -> Path:
    """Default dubbed output folder: sibling named '<folder> Lồng Tiếng'."""
    source = Path(folder)
    name = source.name.strip() or "video"
    parent = source.parent
    if str(parent) in ("", "."):
        parent = source
    return parent / f"{name} Lồng Tiếng"


def _strip_accents(text: str) -> str:
    text = text.replace("đ", "d").replace("Đ", "D")
    text = unicodedata.normalize("NFD", text)
    return "".join(ch for ch in text if unicodedata.category(ch) != "Mn")


def _normalize_text(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = re.sub(r"[\[\]{}]+", " ", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip(" \t\r\n.,;:-")


def _base_words() -> dict[str, str]:
    global _BASE_WORDS
    if _BASE_WORDS is not None:
        return _BASE_WORDS
    mapping: dict[str, tuple[float, str]] = {}
    try:
        from wordfreq import top_n_list, zipf_frequency

        for word in top_n_list("vi", 200000):
            if not word.isalpha():
                continue
            base = _strip_accents(word).lower()
            score = zipf_frequency(word, "vi")
            if base not in mapping or score > mapping[base][0]:
                mapping[base] = (score, word)
    except Exception:
        pass
    _BASE_WORDS = {base: word for base, (_, word) in mapping.items()}
    return _BASE_WORDS


def _best_accent(word: str) -> str:
    cleaned = word.strip()
    if not cleaned:
        return cleaned
    prefix = ""
    suffix = ""
    while cleaned and not cleaned[0].isalnum():
        prefix += cleaned[0]
        cleaned = cleaned[1:]
    while cleaned and not cleaned[-1].isalnum():
        suffix = cleaned[-1] + suffix
        cleaned = cleaned[:-1]
    if not cleaned:
        return prefix + suffix
    candidate = _base_words().get(_strip_accents(cleaned).lower(), cleaned)
    if cleaned[:1].isupper():
        candidate = candidate[:1].upper() + candidate[1:]
    return prefix + candidate + suffix


def _accent_from_easy(rapid_text: str, easy_text: str) -> str:
    """Transfer EasyOCR's diacritics onto RapidOCR's word spacing."""
    rapid_text = _normalize_text(rapid_text)
    easy_text = _normalize_text(easy_text)
    if not rapid_text:
        return easy_text
    if not easy_text:
        return " ".join(_best_accent(word) for word in rapid_text.split())

    rapid_no_space = re.sub(r"\s+", "", rapid_text)
    easy_no_space = re.sub(r"\s+", "", easy_text)
    r_base = _strip_accents(rapid_no_space).lower()
    e_base = _strip_accents(easy_no_space).lower()
    if not r_base or not e_base:
        return rapid_text

    matcher = SequenceMatcher(None, e_base, r_base, autojunk=False)
    mapping: dict[int, int] = {}
    for block in matcher.get_matching_blocks():
        for offset in range(block.size):
            mapping[block.b + offset] = block.a + offset

    result: list[str] = []
    position = 0
    for word in rapid_text.split():
        word_start = position
        position += len(word)
        chars: list[str] = []
        for index in range(len(word)):
            easy_index = mapping.get(word_start + index)
            if easy_index is not None and easy_index < len(easy_no_space):
                chars.append(easy_no_space[easy_index])
            else:
                chars.append(rapid_no_space[word_start + index])
        mapped = sum(
            1
            for index in range(len(word))
            if mapping.get(word_start + index) is not None
        )
        if mapped >= max(1, int(len(word) * 0.6)):
            result.append("".join(chars))
        else:
            result.append(_best_accent(word))
    return " ".join(result)


def _get_easy_reader():
    global _EASY_READER
    if _EASY_READER is None:
        import easyocr

        _EASY_READER = easyocr.Reader(["vi"], gpu=False, verbose=False)
    return _EASY_READER


def _get_vieneu_engine():
    global _VIENEU_ENGINE
    if _VIENEU_ENGINE is None:
        try:
            from vieneu import Vieneu
        except Exception as exc:
            raise RuntimeError(
                "VieNeu-TTS chưa cài. Chạy: pip install vieneu"
            ) from exc
        _VIENEU_ENGINE = Vieneu()
    return _VIENEU_ENGINE


def _ensure_latin_models() -> tuple[Path, Path]:
    base = Path(os.environ.get("LOCALAPPDATA") or Path.home()) / "DramaAI Tiktok Downloader" / "models" / "ocr" / "latin"
    base.mkdir(parents=True, exist_ok=True)
    rec = base / "rec.onnx"
    keys = base / "dict.txt"
    if not rec.is_file() or not keys.is_file():
        import urllib.request

        urls = {
            rec: "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/languages/latin/rec.onnx",
            keys: "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/languages/latin/dict.txt",
        }
        for path, url in urls.items():
            if not path.is_file():
                urllib.request.urlretrieve(url, path)
    return rec, keys


def _get_rapid_engine(latin: bool = False):
    global _RAPID_ENGINE, _RAPID_ENGINE_LATIN
    if latin:
        if _RAPID_ENGINE_LATIN is None:
            try:
                import rapidocr_onnxruntime
                from rapidocr_onnxruntime import RapidOCR

                package_models = Path(rapidocr_onnxruntime.__file__).parent / "models"
                det = package_models / "ch_PP-OCRv4_det_infer.onnx"
                rec, keys = _ensure_latin_models()
                _RAPID_ENGINE_LATIN = RapidOCR(
                    det_model_path=str(det),
                    rec_model_path=str(rec),
                    rec_keys_path=str(keys),
                    det_use_cuda=False,
                    cls_use_cuda=False,
                    rec_use_cuda=False,
                    det_limit_type="max",
                    det_limit_side_len=960,
                    det_unclip_ratio=1.8,
                )
            except Exception:
                _RAPID_ENGINE_LATIN = _get_rapid_engine(False)
        return _RAPID_ENGINE_LATIN

    if _RAPID_ENGINE is None:
        from rapidocr_onnxruntime import RapidOCR

        _RAPID_ENGINE = RapidOCR(
            det_use_cuda=False,
            cls_use_cuda=False,
            rec_use_cuda=False,
            det_limit_type="max",
            det_limit_side_len=960,
            det_unclip_ratio=1.8,
        )
    return _RAPID_ENGINE


def _ffprobe(path: Path) -> dict[str, Any]:
    command = [
        _which("ffprobe"),
        "-v",
        "error",
        "-show_streams",
        "-show_format",
        "-of",
        "json",
        str(path),
    ]
    output = subprocess.check_output(command, stderr=subprocess.STDOUT)
    return json.loads(output.decode("utf-8", errors="replace"))


def _video_info(path: Path) -> tuple[int, int, float, bool]:
    info = _ffprobe(path)
    streams = info.get("streams") or []
    video = next((s for s in streams if s.get("codec_type") == "video"), None)
    if not video:
        raise RuntimeError("File không có stream video.")
    width = int(video.get("width") or 0)
    height = int(video.get("height") or 0)
    duration = float((info.get("format") or {}).get("duration") or video.get("duration") or 0)
    has_audio = any(s.get("codec_type") == "audio" for s in streams)
    if width <= 0 or height <= 0 or duration <= 0:
        raise RuntimeError("Không đọc được kích thước/thời lượng video.")
    return width, height, duration, has_audio


def _read_exact(stream: Any, size: int) -> bytes:
    data = bytearray()
    while len(data) < size:
        chunk = stream.read(size - len(data))
        if not chunk:
            break
        data.extend(chunk)
    return bytes(data)


def _frame_band(video: Path, width: int, height: int) -> Iterable[tuple[float, Any]]:
    import numpy as np

    top = int(height * SUBTITLE_TOP)
    band_height = max(1, int(height * (SUBTITLE_BOTTOM - SUBTITLE_TOP)))
    command = [
        _which("ffmpeg"),
        "-v",
        "error",
        "-i",
        str(video),
        "-vf",
        f"fps={SAMPLE_FPS},crop={width}:{band_height}:0:{top}",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "bgr24",
        "-",
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    frame_size = width * band_height * 3
    index = 0
    try:
        while True:
            chunk = _read_exact(process.stdout, frame_size) if process.stdout else b""
            if not chunk or len(chunk) < frame_size:
                break
            frame = np.frombuffer(chunk, dtype=np.uint8).reshape((band_height, width, 3))
            yield index / SAMPLE_FPS, frame
            index += 1
    finally:
        if process.stdout:
            process.stdout.close()
        if process.poll() is None:
            try:
                process.kill()
            except Exception:
                pass
        process.wait()


def _rapid_lines(
    frame: Any, latin: bool = False
) -> list[tuple[int, str, tuple[int, int, int, int]]]:
    """Fast detection + base text (used for cue timing/spacing)."""
    height, width = frame.shape[:2]
    result, _ = _get_rapid_engine(latin)(frame)
    lines: list[tuple[int, str, tuple[int, int, int, int]]] = []
    for item in result or []:
        try:
            points, text, score = item
            if float(score) < 0.35:
                continue
            xs = [point[0] for point in points]
            ys = [point[1] for point in points]
            x1, x2 = int(min(xs)), int(max(xs))
            y1, y2 = int(min(ys)), int(max(ys))
            if x2 - x1 < width * 0.07 or y2 - y1 < 10:
                continue
            if not re.search(r"[A-Za-zÀ-ỹ]", str(text)):
                continue
            lines.append((y1, _normalize_text(str(text)), (x1, y1, x2, y2)))
        except Exception:
            continue
    lines.sort(key=lambda item: item[0])
    return lines


def _get_vietocr_predictor():
    global _VIETOCR_PREDICTOR
    if _VIETOCR_PREDICTOR is None:
        from vietocr.tool.config import Cfg
        from vietocr.tool.predictor import Predictor

        config = Cfg.load_config_from_name("vgg_transformer")
        config["device"] = "cpu"
        config["predictor"]["beamsearch"] = False
        _VIETOCR_PREDICTOR = Predictor(config)
    return _VIETOCR_PREDICTOR


def _accent_cue_easy(
    frame: Any,
    rapid_lines: list[tuple[int, str, tuple[int, int, int, int]]],
) -> str:
    """Fallback: EasyOCR on the whole subtitle band."""
    rapid_text = " ".join(text for _, text, _ in rapid_lines).strip()
    if not rapid_text or frame is None:
        return rapid_text
    try:
        height, width = frame.shape[:2]
        results = _get_easy_reader().readtext(frame, detail=1, paragraph=False)
        easy_parts: list[tuple[int, str]] = []
        for box, text, score in results or []:
            if float(score) < 0.25:
                continue
            points = list(box)
            xs = [point[0] for point in points]
            ys = [point[1] for point in points]
            if max(xs) - min(xs) < width * 0.05:
                continue
            easy_parts.append((int(min(ys)), str(text)))
        easy_parts.sort(key=lambda item: item[0])
        easy_text = " ".join(text for _, text in easy_parts).strip()
        return _accent_from_easy(rapid_text, easy_text)
    except Exception:
        return " ".join(_best_accent(word) for word in rapid_text.split())


def _accent_cue(
    frame: Any,
    rapid_lines: list[tuple[int, str, tuple[int, int, int, int]]],
) -> str:
    """VietOCR on each detected line, then restore RapidOCR's word spacing."""
    rapid_text = " ".join(text for _, text, _ in rapid_lines).strip()
    if not rapid_text or frame is None:
        return rapid_text
    try:
        from PIL import Image

        predictor = _get_vietocr_predictor()
        height, width = frame.shape[:2]
        accented_lines: list[str] = []
        for _, text, (x1, y1, x2, y2) in rapid_lines:
            pad_x = int((x2 - x1) * 0.03) + 4
            pad_y = int((y2 - y1) * 0.10) + 3
            crop = frame[
                max(0, y1 - pad_y) : min(height, y2 + pad_y),
                max(0, x1 - pad_x) : min(width, x2 + pad_x),
            ]
            if crop.size == 0:
                continue
            image = Image.fromarray(crop[:, :, ::-1].copy())
            viet_text = str(predictor.predict(image)).strip()
            accented_lines.append(_accent_from_easy(text, viet_text))
        if accented_lines:
            return " ".join(accented_lines)
    except Exception:
        pass
    return _accent_cue_easy(frame, rapid_lines)


def _write_srt(cues: list[tuple[float, float, str]], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    chunks: list[str] = []
    for index, (start, end, text) in enumerate(cues, 1):
        def stamp(value: float) -> str:
            millis = int(round(value * 1000))
            hours, millis = divmod(millis, 3_600_000)
            minutes, millis = divmod(millis, 60_000)
            seconds, millis = divmod(millis, 1000)
            return f"{hours:02}:{minutes:02}:{seconds:02},{millis:03}"

        chunks.append(f"{index}\n{stamp(start)} --> {stamp(end)}\n{text}\n")
    path.write_text("\n".join(chunks), encoding="utf-8")


def _parse_srt(text: str) -> list[tuple[float, float, str]]:
    cues: list[tuple[float, float, str]] = []
    blocks = re.split(r"\r?\n\s*\r?\n", text.replace("\r\n", "\n"))
    pattern = re.compile(
        r"(\d{1,2}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{1,2}):(\d{2}):(\d{2})[,.](\d{3})"
    )
    for block in blocks:
        lines = [line.strip() for line in block.split("\n") if line.strip()]
        if not lines:
            continue
        time_line = next((line for line in lines if "-->" in line), None)
        if not time_line:
            continue
        match = pattern.search(time_line)
        if not match:
            continue
        start = (
            int(match.group(1)) * 3600
            + int(match.group(2)) * 60
            + int(match.group(3))
            + int(match.group(4)) / 1000
        )
        end = (
            int(match.group(5)) * 3600
            + int(match.group(6)) * 60
            + int(match.group(7))
            + int(match.group(8)) / 1000
        )
        text_line = " ".join(
            line for line in lines if line is not time_line and not line.isdigit()
        )
        if text_line:
            cues.append((start, end, text_line))
    return cues


def _load_sidecar(video: Path, output_dir: Path) -> Optional[list[tuple[float, float, str]]]:
    candidates = [
        video.with_suffix(".srt"),
        video.with_suffix(".vtt"),
        output_dir / "_phude" / f"{video.stem}.srt",
        output_dir / "_phude" / f"{video.stem}.vtt",
    ]
    for path in candidates:
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
            if path.suffix.lower() == ".vtt":
                text = vtt_to_srt(text)
            cues = _parse_srt(text)
            if cues:
                return cues
        except Exception:
            continue
    return None


def ocr_video(
    video: Path,
    srt_path: Path,
    progress: Optional[Callable[[float, str], None]] = None,
    ocr_mode: str = "quality",
) -> list[tuple[float, float, str]]:
    """OCR burned-in subtitles and write an SRT file."""
    latin = ocr_mode != "quality"
    width, height, duration, _ = _video_info(video)
    cues: list[tuple[float, float, str]] = []
    current_text = ""
    current_frame = None
    current_lines: list[tuple[int, str, tuple[int, int, int, int]]] = []
    start = 0.0
    last_time = 0.0
    sample_count = max(1, int(duration * SAMPLE_FPS))

    def flush(end_time: float) -> None:
        nonlocal current_text, current_frame, current_lines
        if current_text and end_time - start >= 0.4:
            if latin:
                accented = current_text
            else:
                accented = _accent_cue(current_frame, current_lines) or current_text
            cues.append((start, end_time, accented))
        current_text = ""
        current_frame = None
        current_lines = []

    for index, (timestamp, frame) in enumerate(_frame_band(video, width, height)):
        lines = _rapid_lines(frame, latin=latin)
        text = " ".join(item[1] for item in lines).strip()
        last_time = timestamp
        if progress:
            progress(
                min(99.0, (index + 1) / sample_count * 100.0),
                f"OCR {index + 1}/{sample_count}",
            )
        if not text:
            if current_text:
                flush(timestamp)
            continue
        if not current_text:
            current_text = text
            start = timestamp
            current_frame = frame
            current_lines = lines
            continue
        similarity = SequenceMatcher(None, current_text, text, autojunk=False).ratio()
        if similarity < 0.72:
            flush(timestamp)
            current_text = text
            start = timestamp
            current_frame = frame
            current_lines = lines
        else:
            current_text = text
            current_frame = frame
            current_lines = lines

    if current_text:
        flush(min(duration, last_time + 1.0))

    merged: list[tuple[float, float, str]] = []
    for cue in cues:
        if merged:
            prev_start, prev_end, prev_text = merged[-1]
            if (
                cue[0] - prev_end < 0.35
                and SequenceMatcher(None, prev_text, cue[2], autojunk=False).ratio()
                > 0.80
            ):
                merged[-1] = (prev_start, cue[1], cue[2])
                continue
        merged.append(cue)

    _write_srt(merged, srt_path)
    if progress:
        progress(100.0, "Đã tạo SRT")
    return merged


def _clean_speech_text(text: str) -> str:
    text = text.replace("\n", " ")
    text = re.sub(r"\s+", " ", text)
    return text.strip(" .,;:-")


async def _tts_save(
    text: str, voice: str, rate: int, path: Path, provider: str = "edge"
) -> None:
    if provider == "vieneu":
        engine = _get_vieneu_engine()
        audio = engine.infer(text, voice=voice)
        engine.save(audio, str(path))
        _apply_tempo(path, rate)
        return

    import edge_tts

    communicate = edge_tts.Communicate(text, voice, rate=f"{rate:+d}%")
    await communicate.save(str(path))


def _apply_tempo(path: Path, rate: int) -> None:
    """Apply a speed change to a generated WAV using ffmpeg atempo."""
    if rate == 0:
        return
    tempo = max(0.5, min(2.0, 1.0 + rate / 100.0))
    temp = path.with_name(f"{path.stem}_tempo{path.suffix}")
    subprocess.run(
        [
            _which("ffmpeg"),
            "-v",
            "error",
            "-y",
            "-i",
            str(path),
            "-filter:a",
            f"atempo={tempo:.4f}",
            str(temp),
        ],
        check=True,
    )
    temp.replace(path)


def _duration_seconds(path: Path) -> float:
    from pydub import AudioSegment

    return AudioSegment.from_file(path).duration_seconds


def _tts_track(
    cues: list[tuple[float, float, str]],
    video_duration: float,
    voice: str,
    rate: int,
    cache_dir: Path,
    progress: Optional[Callable[[float, str], None]] = None,
    provider: str = "edge",
    soft_timing: bool = False,
    natural_voice: bool = True,
) -> Optional[Path]:
    from pydub import AudioSegment

    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_ext = ".wav" if provider == "vieneu" else ".mp3"
    track = AudioSegment.silent(duration=int(video_duration * 1000), frame_rate=44100)
    used = 0
    previous_cue_end = 0.0
    for index, (start, end, text) in enumerate(cues):
        speech = _clean_speech_text(text)
        if len(speech) < 2:
            previous_cue_end = end
            continue
        cue_duration = max(0.3, end - start)
        key = hashlib.sha1(
            f"{provider}|{voice}|{rate}|{speech}".encode("utf-8")
        ).hexdigest()
        cache = cache_dir / f"{key}{cache_ext}"
        if not cache.is_file():
            try:
                asyncio.run(_tts_save(speech, voice, rate, cache, provider))
            except Exception:
                previous_cue_end = end
                continue
        try:
            segment = AudioSegment.from_file(cache)
            allowed = cue_duration * (1.15 if natural_voice else 1.02)
            if len(segment) / 1000.0 > cue_duration * 1.02:
                faster = max(
                    -30,
                    min(
                        60,
                        rate
                        + int((len(segment) / 1000.0 / cue_duration - 1) * 100)
                        + 5,
                    ),
                )
                if faster != rate:
                    key2 = hashlib.sha1(
                        f"{provider}|{voice}|{faster}|{speech}".encode("utf-8")
                    ).hexdigest()
                    cache2 = cache_dir / f"{key2}{cache_ext}"
                    if not cache2.is_file():
                        asyncio.run(
                            _tts_save(speech, voice, faster, cache2, provider)
                        )
                    segment = AudioSegment.from_file(cache2)
                if len(segment) / 1000.0 > allowed:
                    segment = segment[: int(allowed * 1000)]

            position = start
            if soft_timing and index > 0:
                gap = max(0.0, start - previous_cue_end)
                shift = min(0.25, gap / 2.0)
                position = max(0.0, start - shift)
            track = track.overlay(segment, position=int(position * 1000))
            used += 1
        except Exception:
            previous_cue_end = end
            continue
        previous_cue_end = end
        if progress:
            progress(
                (index + 1) / max(1, len(cues)) * 100.0,
                f"TTS {index + 1}/{len(cues)}",
            )

    if used == 0:
        return None
    path = cache_dir / "track.wav"
    track.export(path, format="wav")
    return path


def _mix_video(
    video: Path,
    tts_track: Path,
    output: Path,
    original_volume: float,
    tts_volume: float,
    has_audio: bool,
) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    command = [_which("ffmpeg"), "-v", "error", "-y", "-i", str(video), "-i", str(tts_track)]
    if has_audio and original_volume > 0:
        filters = (
            f"[0:a]volume={original_volume:.3f}[a0];"
            f"[1:a]volume={tts_volume:.3f}[a1];"
            "[a0][a1]amix=inputs=2:duration=longest:dropout_transition=0,"
            "alimiter=limit=0.95[a]"
        )
    elif tts_volume > 0:
        filters = f"[1:a]volume={tts_volume:.3f},alimiter=limit=0.95[a]"
    else:
        filters = f"[0:a]volume={original_volume:.3f},alimiter=limit=0.95[a]"

    command += [
        "-filter_complex",
        filters,
        "-map",
        "0:v",
        "-map",
        "[a]",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-shortest",
        str(output),
    ]
    subprocess.run(command, check=True)


def dub_video(
    video: Path,
    cues: list[tuple[float, float, str]],
    output: Path,
    voice: str,
    original_volume: int,
    tts_volume: int,
    rate: int,
    cache_dir: Path,
    overwrite: bool,
    progress: Optional[Callable[[float, str], None]] = None,
    tts_provider: str = "edge",
    soft_timing: bool = False,
    natural_voice: bool = True,
) -> None:
    if output.exists() and not overwrite:
        return
    width, height, duration, has_audio = _video_info(video)
    track = _tts_track(
        cues,
        duration,
        voice,
        rate,
        cache_dir,
        progress=lambda p, m: progress(p * 0.8, m) if progress else None,
        provider=tts_provider,
        soft_timing=soft_timing,
        natural_voice=natural_voice,
    )
    if track is None:
        raise RuntimeError("Không tạo được audio TTS.")
    if progress:
        progress(85.0, "Đang mix audio")
    _mix_video(
        video,
        track,
        output,
        max(0.0, min(1.0, original_volume / 100.0)),
        max(0.0, min(2.0, tts_volume / 100.0)),
        has_audio,
    )
    if progress:
        progress(100.0, "Đã mix xong")


def run_batch(
    folder: str,
    output_dir: str,
    voice: str,
    original_volume: int,
    tts_volume: int,
    rate: int,
    use_sidecar: bool,
    ocr_only: bool,
    overwrite: bool,
    emit: Callable[..., None],
    ocr_mode: str = "quality",
    tts_provider: str = "edge",
    soft_timing: bool = False,
    natural_voice: bool = True,
    keep_temp: bool = False,
) -> None:
    videos = list_videos(folder)
    if not videos:
        raise RuntimeError("Không tìm thấy video trong thư mục.")
    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    srt_dir = output_root / "_phude"
    srt_dir.mkdir(parents=True, exist_ok=True)
    cache_dir = output_root / "_tts_cache"
    cache_dir.mkdir(parents=True, exist_ok=True)

    emit("dub_start", total=len(videos), folder=str(Path(folder)), output_dir=str(output_root))
    if tts_provider == "vieneu":
        emit(
            "dub_progress",
            index=1,
            name="",
            stage="tts",
            percent=0.0,
            message="Đang khởi tạo VieNeu-TTS (lần đầu có thể tải model)...",
        )
        try:
            _get_vieneu_engine()
        except Exception as exc:
            raise RuntimeError(f"VieNeu-TTS chưa sẵn sàng: {exc}") from exc
    done = 0
    failed = 0
    started = time.time()

    for index, video in enumerate(videos, 1):
        emit("dub_video_start", index=index, total=len(videos), name=video.name)
        try:
            output_video = output_root / f"{video.stem}.mp4"
            if output_video.exists() and not overwrite and not ocr_only:
                emit("dub_video_done", index=index, name=video.name, output=str(output_video), skipped=True)
                done += 1
                continue

            srt_path = srt_dir / f"{video.stem}.srt"
            cues = _load_sidecar(video, output_root) if use_sidecar else None
            if cues is None:
                cues = ocr_video(
                    video,
                    srt_path,
                    progress=lambda p, m: emit(
                        "dub_progress", index=index, name=video.name, stage="ocr", percent=round(p, 1), message=m
                    ),
                    ocr_mode=ocr_mode,
                )
            elif not srt_path.exists():
                _write_srt(cues, srt_path)

            if not cues:
                raise RuntimeError("OCR không đọc được phụ đề.")

            if ocr_only:
                emit("dub_video_done", index=index, name=video.name, output=str(srt_path), skipped=False)
                done += 1
                continue

            def report(p: float, m: str, _index=index, _name=video.name):
                emit(
                    "dub_progress",
                    index=_index,
                    name=_name,
                    stage="tts",
                    percent=round(p, 1),
                    message=m,
                )

            dub_video(
                video,
                cues,
                output_video,
                voice,
                original_volume,
                tts_volume,
                rate,
                cache_dir,
                overwrite,
                progress=report,
                tts_provider=tts_provider,
                soft_timing=soft_timing,
                natural_voice=natural_voice,
            )
            emit("dub_video_done", index=index, name=video.name, output=str(output_video), skipped=False)
            done += 1
        except Exception as exc:
            failed += 1
            emit("dub_video_error", index=index, name=video.name, message=str(exc))

    if not keep_temp and not ocr_only and done > 0:
        shutil.rmtree(srt_dir, ignore_errors=True)
        shutil.rmtree(cache_dir, ignore_errors=True)
        emit(
            "log",
            message="Đã xóa file tạm _phude và _tts_cache; chỉ giữ video lồng tiếng.",
        )

    emit(
        "dub_finished",
        done=done,
        failed=failed,
        total=len(videos),
        elapsed=round(time.time() - started, 1),
    )
