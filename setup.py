from setuptools import setup, find_packages

setup(
    name="dramaai-tiktok-downloader",
    version="1.3.1",
    description="DramaAI Tiktok Downloader - Multi-Platform Short Drama Extractor and Downloader Module",
    packages=find_packages(),
    install_requires=[
        "requests>=2.28.0",
        "cryptography>=41.0.0",
    ],
    extras_require={
        # Batch OCR hardsub + Vietnamese TTS + audio mix (tab "Lồng tiếng").
        # ffmpeg must be available on PATH. VietOCR downloads its model on first use.
        "dubbing": [
            "edge-tts>=7.0.0",
            "pydub>=0.25.1",
            "numpy>=1.24.0",
            "opencv-python>=4.8.0",
            "rapidocr-onnxruntime>=1.3.0",
            "vietocr>=0.3.13",
            "wordfreq>=3.1.1",
            "setuptools<81",
        ],
        # Optional fallback OCR when VietOCR is unavailable.
        "dubbing-easyocr": [
            "easyocr>=1.7.0",
        ],
        # Optional local 48 kHz Vietnamese TTS with 25 preset voices + cloning.
        # Requires Python 3.10+; models download on first use.
        "dubbing-vieneu": [
            "vieneu>=3.8.4",
        ],
        # Optional local ZeroTTS (MIT, ONNX CPU, 8 Vietnamese voices).
        "dubbing-zerotts": [
            "zerotts>=0.1.5",
        ],
        # Optional free online provider (Google Translate TTS).
        "dubbing-gtts": [
            "gTTS>=2.5.0",
        ],
        # Optional Google Cloud TTS (needs a service-account JSON).
        "dubbing-google": [
            "google-cloud-texttospeech>=2.16.0",
        ],
    },
    entry_points={
        "console_scripts": [
            "dramaai=drama_downloader.cli:main",
            "dramadl=drama_downloader.cli:main",
        ],
    },
    python_requires=">=3.8",
)
