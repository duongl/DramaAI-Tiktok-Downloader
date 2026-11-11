"""
drama_downloader.merge
~~~~~~~~~~~~~~~~~~~~~~
Ghép toàn bộ video trong một thư mục thành 1 file duy nhất.

Cách làm: dùng ffmpeg concat demuxer với ``-c copy`` — không render lại,
không mã hoá lại nên rất nhanh, chỉ nối các đoạn video/audio gốc.

Chỉ dùng cho video bạn sở hữu hoặc có quyền xử lý.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Callable, Optional

from .dubbing import _which, list_videos

_EPISODE_RE = re.compile(r"(?:EP|TẬP|TAP|EPISODE)?\s*0*(\d+)", re.IGNORECASE)


def natural_sort_key(path: Path) -> tuple[int, int, str]:
    """Sort EP01, EP02, ... EP10 đúng thứ tự số tập."""
    stem = path.stem.strip()
    match = _EPISODE_RE.search(stem)
    if match:
        return (0, int(match.group(1)), stem.lower())
    return (1, 0, stem.lower())


def merge_list(folder: str | Path) -> list[Path]:
    """Return the videos to merge, sorted by episode number."""
    return sorted(list_videos(folder), key=natural_sort_key)


def default_merge_output(folder: str | Path) -> Path:
    """Default output: sibling ``<tên thư mục> Full.mp4``."""
    source = Path(folder)
    name = source.name.strip() or "video"
    parent = source.parent
    if str(parent) in ("", "."):
        parent = source
    return parent / f"{name} Full.mp4"


def _ffprobe_duration(path: Path) -> float:
    try:
        output = subprocess.check_output(
            [
                _which("ffprobe"),
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "json",
                str(path),
            ],
            stderr=subprocess.STDOUT,
        )
        data = json.loads(output.decode("utf-8", errors="replace"))
        return float((data.get("format") or {}).get("duration") or 0.0)
    except Exception:
        return 0.0


def _concat_entry(path: Path) -> str:
    escaped = str(path.resolve()).replace("\\", "/").replace("'", "'\\''")
    return f"file '{escaped}'"


def _run_ffmpeg(command: list[str]) -> None:
    subprocess.run(command, check=True)


def merge_folder(
    folder: str | Path,
    output: str | Path,
    overwrite: bool = True,
    reencode: bool = False,
    emit: Optional[Callable[..., None]] = None,
) -> dict[str, Any]:
    """Merge every video in *folder* into *output*.

    Tries a stream copy first (fast). If that fails and *reencode* is True,
    falls back to a re-encode pass.
    """
    def report(event: str, **payload: Any) -> None:
        if emit:
            emit(event, **payload)

    videos = merge_list(folder)
    if not videos:
        raise RuntimeError("Không tìm thấy video trong thư mục.")

    output_path = Path(output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if output_path.exists() and not overwrite:
        raise RuntimeError(f"File đã tồn tại: {output_path}. Bật \"Ghi đè\" để ghi lại.")

    started = time.time()
    report(
        "merge_start",
        total=len(videos),
        folder=str(Path(folder)),
        output=str(output_path),
        names=[video.name for video in videos[:3]],
    )

    fd, list_name = tempfile.mkstemp(prefix="dramaai_merge_", suffix=".txt")
    os.close(fd)
    list_file = Path(list_name)
    try:
        list_file.write_text(
            "\n".join(_concat_entry(video) for video in videos),
            encoding="utf-8",
        )
        report("merge_progress", percent=10.0, message=f"Đang nối {len(videos)} video (copy)...")

        base = [
            _which("ffmpeg"),
            "-v",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(list_file),
        ]
        copy_command = base + [
            "-c",
            "copy",
            "-map",
            "0",
        ]
        if output_path.suffix.lower() in (".mp4", ".m4v", ".mov"):
            copy_command += ["-movflags", "+faststart"]
        copy_command.append(str(output_path))

        try:
            _run_ffmpeg(copy_command)
        except subprocess.CalledProcessError:
            if not reencode:
                raise RuntimeError(
                    "Không nối trực tiếp được (các tập khác codec/độ phân giải). "
                    "Bật \"Ghép lại nếu lỗi\" rồi thử lại."
                )
            report(
                "merge_progress",
                percent=40.0,
                message="Copy lỗi, đang ghép lại (re-encode)...",
            )
            reencode_command = base + [
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                str(output_path),
            ]
            _run_ffmpeg(reencode_command)

        elapsed = round(time.time() - started, 1)
        size = output_path.stat().st_size if output_path.is_file() else 0
        duration = _ffprobe_duration(output_path)
        report(
            "merge_done",
            output=str(output_path),
            size=size,
            duration=round(duration, 1),
            elapsed=elapsed,
        )
        report("merge_finished", done=len(videos), elapsed=elapsed)
        return {
            "output": str(output_path),
            "videos": len(videos),
            "size": size,
            "duration": duration,
            "elapsed": elapsed,
        }
    finally:
        try:
            list_file.unlink(missing_ok=True)
        except Exception:
            pass
