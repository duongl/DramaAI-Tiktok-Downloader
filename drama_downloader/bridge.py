"""
drama_downloader.bridge
~~~~~~~~~~~~~~~~~~~~~~~
JSON bridge between the Rust GUI and the Python extraction/download core.

Protocol
--------
The process reads exactly one JSON request from stdin and writes JSON lines
to stdout. Every line is an independent event::

    {"event": "series", "series": {...}}
    {"event": "episode_start", "num": 1, "title": "..."}
    {"event": "progress", "num": 1, "percent": 42.5, ...}
    {"event": "episode_done", "num": 1, "path": "..."}
    {"event": "episode_error", "num": 1, "message": "..."}
    {"event": "finished", "ok": 10, "failed": 0}
    {"event": "error", "message": "..."}

Requests::

    {"cmd": "info", "target": "https://...", "platform": null}
    {"cmd": "download", "target": "...", "platform": null,
     "output": "./downloads", "concurrency": 3, "save_vtt": false,
     "episodes": [1, 2, 3]}
    {"cmd": "catalog", "platform": "dramabox", "page": 1, "query": null}
"""

import json
import sys
import threading
import time
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

try:
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")
except Exception:
    pass

_emit_lock = threading.Lock()


def emit(event, **payload):
    """Write one protocol line. Safe to call from worker threads."""
    data = {"event": event}
    data.update(payload)
    line = json.dumps(data, ensure_ascii=False, default=str)
    with _emit_lock:
        sys.stdout.write(line + "\n")
        sys.stdout.flush()


def series_payload(series):
    return {
        "name": series.name or "",
        "platform": series.platform or "",
        "series_id": series.series_id or "",
        "cover": series.cover or "",
        "description": series.description or "",
        "episode_count": int(series.episode_count or 0),
        "accessible_count": int(series.accessible_count or 0),
        "url": series.url or "",
        "episodes": [
            {
                "num": int(ep.num),
                "title": ep.title or "",
                "locked": bool(ep.locked),
                "has_stream": bool(ep.direct_url),
                "subs": len(ep.subs or []),
            }
            for ep in series.episodes
        ],
    }


def cmd_info(req):
    from drama_downloader import load_series

    target = (req.get("target") or "").strip()
    if not target:
        raise ValueError("Chưa nhập link phim hoặc ID.")
    series = load_series(target, platform_hint=req.get("platform") or None)
    emit("series", series=series_payload(series))
    return 0


def cmd_download(req):
    from drama_downloader import DramaDownloader, load_series

    target = (req.get("target") or "").strip()
    if not target:
        raise ValueError("Chưa nhập link phim hoặc ID.")

    series = load_series(target, platform_hint=req.get("platform") or None)
    emit("series", series=series_payload(series))

    episodes = req.get("episodes")
    if episodes is not None:
        if not isinstance(episodes, list) or not episodes:
            raise ValueError("Chưa chọn tập nào để tải.")
        wanted = {int(n) for n in episodes}
        targets = [ep for ep in series.episodes if int(ep.num) in wanted]
        if not targets:
            raise ValueError("Không có tập nào trong danh sách đã chọn.")
        episodes_range = sorted(int(n) for n in wanted)
    else:
        targets = list(series.episodes)
        episodes_range = None

    output_dir = req.get("output") or "./downloads"
    concurrency = max(1, min(8, int(req.get("concurrency") or 2)))
    save_vtt = bool(req.get("save_vtt"))

    downloader = DramaDownloader(
        output_dir=output_dir,
        concurrency=concurrency,
        save_vtt=save_vtt,
    )

    emit(
        "start",
        total=len(targets),
        output_dir=str(downloader.output_dir),
        series_dir=downloader.series_output_dir(series),
    )

    last_emit = {}
    started_at = time.time()

    def on_start(ep):
        emit("episode_start", num=int(ep.num), title=ep.title or "")

    def on_progress(ep, progress):
        now = time.time()
        previous = last_emit.get(ep.num, 0.0)
        percent = float(progress.percent)
        if percent < 100.0 and now - previous < 0.25:
            return
        last_emit[ep.num] = now
        emit(
            "progress",
            num=int(ep.num),
            percent=round(percent, 1),
            downloaded=int(progress.downloaded_bytes),
            total=int(progress.total_bytes),
            speed=round(float(progress.speed), 1),
        )

    def on_done(ep, path):
        emit("episode_done", num=int(ep.num), path=str(path))

    def on_error(ep, err):
        emit("episode_error", num=int(ep.num), message=str(err))

    completed = downloader.download_series(
        series,
        episodes_range=episodes_range,
        on_episode_start=on_start,
        on_episode_done=on_done,
        on_episode_error=on_error,
        on_episode_progress=on_progress,
    )

    emit(
        "finished",
        ok=len(completed),
        failed=max(0, len(targets) - len(completed)),
        elapsed=round(time.time() - started_at, 1),
        paths=[str(p) for p in completed],
    )
    return 0


def cmd_catalog(req):
    from drama_downloader import get_platform

    platform_id = (req.get("platform") or "").strip()
    platform = get_platform(platform_id) if platform_id else None
    if not platform:
        raise ValueError(f"Nền tảng không hợp lệ: {platform_id}")

    result = platform.get_catalog(
        page=max(1, int(req.get("page") or 1)),
        query=req.get("query") or None,
        category=req.get("category") or None,
    )
    emit(
        "catalog",
        platform=platform.platform_id,
        total=int(result.total or 0),
        items=[item.to_dict() for item in result.items],
    )
    return 0


def cmd_dub(req):
    from drama_downloader import dubbing

    folder = (req.get("folder") or "").strip()
    if not folder or not Path(folder).is_dir():
        raise ValueError("Thư mục video không hợp lệ.")

    output = (req.get("output") or "").strip()
    if not output:
        output = str(dubbing.default_output_dir(folder))
    voice = (req.get("voice") or "vi-VN-HoaiMyNeural").strip()
    original_volume = max(0, min(100, int(req.get("original_volume") or 15)))
    tts_volume = max(0, min(200, int(req.get("tts_volume") or 100)))
    rate = max(-50, min(50, int(req.get("rate") or 0)))
    use_sidecar = bool(req.get("use_sidecar", True))
    ocr_only = bool(req.get("ocr_only", False))
    overwrite = bool(req.get("overwrite", False))
    ocr_mode = req.get("ocr_mode") or "quality"
    tts_provider = (req.get("tts_provider") or "edge").strip().lower()
    if tts_provider not in ("edge", "vieneu"):
        raise ValueError(f"TTS provider không hợp lệ: {tts_provider}")
    soft_timing = bool(req.get("soft_timing", False))
    natural_voice = bool(req.get("natural_voice", True))
    keep_temp = bool(req.get("keep_temp", False))

    dubbing.run_batch(
        folder=folder,
        output_dir=output,
        voice=voice,
        original_volume=original_volume,
        tts_volume=tts_volume,
        rate=rate,
        use_sidecar=use_sidecar,
        ocr_only=ocr_only,
        overwrite=overwrite,
        emit=emit,
        ocr_mode=ocr_mode,
        tts_provider=tts_provider,
        soft_timing=soft_timing,
        natural_voice=natural_voice,
        keep_temp=keep_temp,
    )
    return 0


def main():
    raw = sys.stdin.buffer.read().decode("utf-8", errors="replace")
    try:
        req = json.loads(raw) if raw.strip() else {}
    except Exception as exc:
        emit("error", message=f"JSON không hợp lệ: {exc}")
        return 1

    command = req.get("cmd")
    try:
        if command == "info":
            return cmd_info(req)
        if command == "download":
            return cmd_download(req)
        if command == "catalog":
            return cmd_catalog(req)
        if command == "dub":
            return cmd_dub(req)
        raise ValueError(f"Lệnh không hỗ trợ: {command!r}")
    except Exception as exc:
        emit(
            "error",
            message=str(exc) or exc.__class__.__name__,
            traceback=traceback.format_exc(),
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
