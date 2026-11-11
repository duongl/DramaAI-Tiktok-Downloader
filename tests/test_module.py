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
from drama_downloader import dubbing
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
        self.assertEqual(len(dubbing.VIENEU_VOICES), 25)
        self.assertEqual(len(set(dubbing.VIENEU_VOICES)), 25)
        self.assertIn("Hải Đăng", dubbing.VIENEU_VOICES)
        self.assertEqual(len(dubbing.VI_VOICES), 2)
        import inspect

        params = inspect.signature(dubbing.run_batch).parameters
        self.assertIn("tts_provider", params)
        self.assertIn("soft_timing", params)
        self.assertIn("natural_voice", params)
        self.assertIn("keep_temp", params)
        self.assertEqual(
            dubbing.default_output_dir(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu"),
            Path(r"C:\Phim AI\Yêu Đương Chẳng Bằng Đi Ăn Lẩu Lồng Tiếng"),
        )
        print("[OK] TTS voice catalog test passed")


if __name__ == "__main__":
    unittest.main()
