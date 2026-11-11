"""
test_module.py
~~~~~~~~~~~~~~
Verification test suite for drama_downloader package.
"""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
import drama_downloader as dd
from drama_downloader import dubbing, merge
from drama_downloader.downloader import (
    DramaDownloader,
    sanitize_filename,
    series_folder_name,
)


class TestDramaDownloader(unittest.TestCase):

    def test_rsa_signing(self):
        sig = dd.sign_dramabox("hello")
        self.assertTrue(len(sig) > 100)
        self.assertTrue(sig.startswith("e1Womj1QYXmFgI1NcC9bUFl5LIQLmUV9hjtFjwct"))
        print("[OK] RSA signing test passed (signature identical to Node.js)")

    def test_platform_detection(self):
        cases = [
            ("https://www.dramaboxdb.com/movie/42000001234/drama", "dramabox"),
            ("https://netshort.com/drama/test12345", "netshort"),
            ("https://kisskh.co/Drama/My-Title?id=1234", "kisskh"),
            ("https://www.shortmax.com/drama/sm12345", "shortmax"),
            ("https://freereels.app/drama/fr123456", "freereels"),
            ("https://hongguoduanju.com/detail?series_id=74123456789", "hongguo"),
            ("https://www.dramawave.com/drama/dw12345", "dramawave"),
            ("https://raptdrama.com/drama/12345", "raptdrama"),
            ("https://flextv.com/drama/fx12345", "flextv"),
            ("https://stardusttv.com/video/12345", "stardusttv"),
        ]
        for url, expected in cases:
            plat = dd.detect_platform(url)
            self.assertIsNotNone(plat, f"Failed to detect platform for {url}")
            self.assertEqual(plat.platform_id, expected)
        print(f"[OK] All {len(cases)} platform detection tests passed")

    def test_platforms_list(self):
        platforms = dd.get_all_platforms()
        self.assertGreaterEqual(len(platforms), 11)
        print(f"[OK] Total active platforms: {len(platforms)}")

    def test_vtt_to_srt(self):
        sample_vtt = """WEBVTT

00:00:01.000 --> 00:00:03.500
Hello World!

00:00:04.000 --> 00:00:06.000
<b>Welcome to DramaAI</b>
"""
        srt = dd.vtt_to_srt(sample_vtt)
        self.assertIn("00:00:01,000 --> 00:00:03,500", srt)
        self.assertIn("1\n", srt)
        self.assertIn("Hello World!", srt)
        self.assertIn("Welcome to DramaAI", srt)
        print("[OK] VTT to SRT subtitle conversion test passed")

    def test_series_folder_names(self):
        self.assertEqual(sanitize_filename('Phim: A/B?'), "Phim A B")
        self.assertEqual(sanitize_filename("CON"), "_CON")
        self.assertEqual(sanitize_filename("   "), "video")

        class FakeSeries:
            name = "Yêu Đương Chẳng Bằng Đi Ăn Lẩu"
            series_id = "dramabox:123"

        self.assertEqual(
            series_folder_name(FakeSeries()), "Yêu Đương Chẳng Bằng Đi Ăn Lẩu"
        )
        FakeSeries.name = ""
        self.assertEqual(series_folder_name(FakeSeries()), "dramabox 123")
        print("[OK] Series folder naming test passed")

    def test_series_output_dir(self):
        class FakeSeries:
            name = "Yêu Đương Chẳng Bằng Đi Ăn Lẩu"
            series_id = "dramabox:123"

        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            downloader = DramaDownloader(output_dir=str(base))
            self.assertEqual(
                Path(downloader.series_output_dir(FakeSeries())),
                base / "Yêu Đương Chẳng Bằng Đi Ăn Lẩu",
            )

            series_folder = base / "Yêu Đương Chẳng Bằng Đi Ăn Lẩu"
            series_folder.mkdir()
            nested = DramaDownloader(output_dir=str(series_folder))
            self.assertEqual(
                Path(nested.series_output_dir(FakeSeries())), series_folder
            )
        print("[OK] Series output directory test passed")

    def test_dubbing_helpers(self):
        self.assertEqual(dubbing._strip_accents("Đẹp"), "Dep")
        self.assertEqual(dubbing._normalize_text("  <i>Xin chào</i>  "), "Xin chào")

        cues = [(1.5, 3.0, "Xin chào"), (3.5, 5.25, "Tạm biệt")]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            srt_path = root / "sample.srt"
            dubbing._write_srt(cues, srt_path)
            parsed = dubbing._parse_srt(srt_path.read_text(encoding="utf-8"))
            self.assertEqual(len(parsed), 2)
            self.assertAlmostEqual(parsed[0][0], 1.5)
            self.assertAlmostEqual(parsed[1][1], 5.25)
            self.assertEqual(parsed[1][2], "Tạm biệt")

            (root / "EP01.mp4").write_bytes(b"")
            (root / "EP02.MKV").write_bytes(b"")
            (root / "note.txt").write_text("x", encoding="utf-8")
            self.assertEqual(
                [path.name for path in dubbing.list_videos(root)],
                ["EP01.mp4", "EP02.MKV"],
            )
        print("[OK] Dubbing helper tests passed")

    def test_tts_voice_catalog(self):
        self.assertIn("edge", dubbing.TTS_PROVIDERS)
        self.assertIn("vieneu", dubbing.TTS_PROVIDERS)
        self.assertIn("zerotts", dubbing.TTS_PROVIDERS)
        self.assertIn("gtts", dubbing.TTS_PROVIDERS)
        self.assertIn("elevenlabs", dubbing.TTS_PROVIDERS)
        self.assertIn("openai", dubbing.TTS_PROVIDERS)
        self.assertIn("google", dubbing.TTS_PROVIDERS)
        self.assertIn("azure", dubbing.TTS_PROVIDERS)
        self.assertEqual(len(dubbing.VIENEU_VOICES), 25)
        self.assertEqual(len(set(dubbing.VIENEU_VOICES)), 25)
        self.assertIn("Hải Đăng", dubbing.VIENEU_VOICES)
        self.assertEqual(len(dubbing.ZEROTTS_VOICES), 8)
        self.assertEqual(dubbing.ZEROTTS_VOICES[0][0], "maichi")
        self.assertEqual(len(dubbing.VI_VOICES), 2)
        self.assertGreaterEqual(len(dubbing.OPENAI_VOICES), 5)
        self.assertGreaterEqual(len(dubbing.GOOGLE_VOICES), 4)
        self.assertGreaterEqual(len(dubbing.AZURE_VOICES), 2)
        import inspect

        params = inspect.signature(dubbing.run_batch).parameters
        self.assertIn("tts_provider", params)
        self.assertIn("soft_timing", params)
        self.assertIn("natural_voice", params)
        self.assertIn("keep_temp", params)
        self.assertIn("tts_config", params)
        self.assertEqual(
            dubbing.default_output_dir(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu"),
            Path(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu Lồng Tiếng"),
        )
        print("[OK] TTS voice catalog test passed")

    def test_tts_preflight_errors(self):
        with self.assertRaises(RuntimeError) as ctx:
            dubbing._preflight_provider("elevenlabs", {})
        self.assertIn("ElevenLabs", str(ctx.exception))
        with self.assertRaises(RuntimeError) as ctx:
            dubbing._preflight_provider("openai", {"api_key": ""})
        self.assertIn("OpenAI", str(ctx.exception))
        with self.assertRaises(RuntimeError) as ctx:
            dubbing._preflight_provider("azure", {"api_key": "x", "region": ""})
        self.assertIn("Azure", str(ctx.exception))
        with self.assertRaises(RuntimeError) as ctx:
            dubbing._preflight_provider("google", {"credentials": "missing.json"})
        self.assertIn("credentials", str(ctx.exception))
        print("[OK] TTS preflight test passed")

    def test_merge_helpers(self):
        paths = [
            Path("EP10 - 第十集.mp4"),
            Path("EP2 - 第二集.mp4"),
            Path("EP1 - 第一集.mp4"),
        ]
        ordered = sorted(paths, key=merge.natural_sort_key)
        self.assertEqual(
            [path.name for path in ordered],
            ["EP1 - 第一集.mp4", "EP2 - 第二集.mp4", "EP10 - 第十集.mp4"],
        )
        self.assertEqual(
            merge.default_merge_output(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu"),
            Path(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu Full.mp4"),
        )
        print("[OK] Merge helper test passed")

    def test_merge_copy(self):
        import shutil
        import subprocess

        if shutil.which("ffmpeg") is None:
            self.skipTest("ffmpeg not available")
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            source = base / "Demo Series"
            source.mkdir()
            for index, name in enumerate(["EP1 - 第一集", "EP2 - 第二集"]):
                subprocess.run(
                    [
                        "ffmpeg",
                        "-v",
                        "error",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc=size=160x120:rate=10:duration=1",
                        "-f",
                        "lavfi",
                        "-i",
                        "sine=frequency=440:duration=1",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-c:a",
                        "aac",
                        "-shortest",
                        str(source / f"{name}.mp4"),
                    ],
                    check=True,
                )
            output = merge.default_merge_output(source)
            result = merge.merge_folder(
                source, output, overwrite=True, reencode=False, emit=lambda *a, **k: None
            )
            self.assertTrue(output.is_file())
            self.assertGreater(result["size"], 0)
            self.assertGreater(result["duration"], 1.8)
        print("[OK] Merge copy test passed")


if __name__ == "__main__":
    unittest.main()
