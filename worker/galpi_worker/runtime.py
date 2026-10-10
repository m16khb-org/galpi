"""Shared runtime configuration for third-party audio libraries."""

import gc
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


def failure_reason(error: BaseException) -> str:
    """One short line naming why an accelerator step failed, for the log."""

    lines = str(error).strip().splitlines()
    detail = lines[0][:200] if lines else ""
    return f"{type(error).__name__}: {detail}" if detail else type(error).__name__


def release_torch_cache() -> None:
    """Hand cached accelerator memory back before the next model loads.

    PyTorch keeps freed MPS/CUDA blocks in its own pool. After WhisperX
    alignment of a 40-minute meeting the MPS pool held about 12 GB, which on
    a 16 GB Mac left too little for diarization and failed it over to CPU.
    """

    gc.collect()
    torch = import_module("torch")
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()


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
