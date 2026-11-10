from setuptools import setup, find_packages

setup(
    name="dramaai-tiktok-downloader",
    version="1.1.0",
    description="DramaAI Tiktok Downloader - Multi-Platform Short Drama Extractor and Downloader Module",
    packages=find_packages(),
    install_requires=[
        "requests>=2.28.0",
        "cryptography>=41.0.0",
    ],
    entry_points={
        "console_scripts": [
            "dramaai=drama_downloader.cli:main",
            "dramadl=drama_downloader.cli:main",
        ],
    },
    python_requires=">=3.8",
)
