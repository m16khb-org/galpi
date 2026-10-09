"""Shared runtime configuration for third-party audio libraries."""

import logging
import sys
import warnings
from dataclasses import dataclass
from importlib import import_module
from importlib.util import find_spec
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


AsrBackend = Literal["mlx", "ctranslate2"]


@dataclass(frozen=True, slots=True)
class AsrRuntime:
    """Which library decodes speech, on which device, at which precision."""

    backend: AsrBackend
    device: str
    compute_type: str

    def label(self) -> str:
        if self.backend == "mlx":
            return "MLX float16"
        return f"CTranslate2 {self.device.upper()} {self.compute_type}"


CPU_ASR = AsrRuntime("ctranslate2", "cpu", "int8")


class Ctranslate2(Protocol):
    @staticmethod
    def get_cuda_device_count() -> int: ...


def select_asr_runtime(
    torch_device: TorchDevice, *, mlx_available: bool, ct2_cuda_devices: int
) -> AsrRuntime:
    """Pick the ASR runtime; the rest of the WhisperX pipeline is shared.

    MLX (Metal) wherever `mlx-whisper` is installed, which only the macOS lock
    does: CTranslate2 has no Metal backend. CUDA float16 when torch runs on
    CUDA (the CUDA build's torch wheels also carry the cuBLAS/cuDNN libraries
    CTranslate2 loads) and CTranslate2 sees a GPU. CPU int8 otherwise.
    """

    if mlx_available:
        return AsrRuntime("mlx", "mlx", "float16")
    if torch_device == "cuda" and ct2_cuda_devices > 0:
        return AsrRuntime("ctranslate2", "cuda", "float16")
    return CPU_ASR


def detect_asr_runtime(torch_device: TorchDevice) -> AsrRuntime:
    """ASR runtime for this machine. Heavy modules are imported lazily."""

    mlx_available = find_spec("mlx_whisper") is not None
    if mlx_available or torch_device != "cuda":
        return select_asr_runtime(
            torch_device, mlx_available=mlx_available, ct2_cuda_devices=0
        )
    ctranslate2 = cast(Ctranslate2, cast(object, import_module("ctranslate2")))
    return select_asr_runtime(
        torch_device,
        mlx_available=False,
        ct2_cuda_devices=ctranslate2.get_cuda_device_count(),
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
