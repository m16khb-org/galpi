"""Shared runtime configuration for third-party audio libraries."""

import logging
import sys
import warnings
from importlib import import_module
from typing import Literal, Protocol, cast

TorchDevice = Literal["cpu", "mps", "cuda"]


class ImageioFfmpeg(Protocol):
    @staticmethod
    def get_ffmpeg_exe() -> str: ...


def ffmpeg_executable() -> str:
    """Path to the ffmpeg binary bundled with imageio-ffmpeg.

    Imported lazily: the pure helpers in this package must stay importable
    without the engine virtualenv installed.
    """

    return cast(
        ImageioFfmpeg,
        cast(object, import_module("imageio_ffmpeg")),
    ).get_ffmpeg_exe()


def select_torch_device(
    *, mps_available: bool, cuda_available: bool = False
) -> TorchDevice:
    if cuda_available:
        return "cuda"
    return "mps" if mps_available else "cpu"


def detect_torch_device() -> TorchDevice:
    """Best available torch device. torch is imported lazily."""

    torch = import_module("torch")
    return select_torch_device(
        mps_available=bool(torch.backends.mps.is_available()),
        cuda_available=bool(torch.cuda.is_available()),
    )


def needs_cpu_fallback(device: TorchDevice) -> bool:
    """True when a failed accelerator step should be retried once on CPU."""

    return device != "cpu"


AsrDevice = Literal["cpu", "cuda"]


class Ctranslate2(Protocol):
    @staticmethod
    def get_cuda_device_count() -> int: ...


def select_asr_runtime(
    torch_device: TorchDevice, *, ct2_cuda_devices: int
) -> tuple[AsrDevice, str]:
    """CTranslate2 device and compute type for ASR.

    CUDA float16 when torch runs on CUDA (the Windows CUDA build, whose torch
    wheels also carry the cuBLAS/cuDNN DLLs CTranslate2 loads) and CTranslate2
    sees a GPU. CTranslate2 has no MPS backend, so every other case keeps CPU
    int8.
    """

    if torch_device == "cuda" and ct2_cuda_devices > 0:
        return "cuda", "float16"
    return "cpu", "int8"


def detect_asr_runtime(torch_device: TorchDevice) -> tuple[AsrDevice, str]:
    """ASR runtime for this machine. ctranslate2 is imported lazily."""

    if torch_device != "cuda":
        return select_asr_runtime(torch_device, ct2_cuda_devices=0)
    ctranslate2 = cast(Ctranslate2, cast(object, import_module("ctranslate2")))
    return select_asr_runtime(
        torch_device, ct2_cuda_devices=ctranslate2.get_cuda_device_count()
    )


def ffmpeg_link_name(platform: str = sys.platform) -> str:
    # Keep in sync with Rust `Os::ffmpeg_file_name()` in
    # src-tauri/src/adapters/outbound/platform.rs.
    return "ffmpeg.exe" if platform == "win32" else "ffmpeg"


def configure_warnings() -> None:
    logging.getLogger("lightning.pytorch.utilities.migration.utils").setLevel(
        logging.WARNING
    )
    warnings.filterwarnings(
        "ignore",
        message=r"\ntorchcodec is not installed correctly",
        category=UserWarning,
        module=r"pyannote\.audio\.core\.io",
    )
