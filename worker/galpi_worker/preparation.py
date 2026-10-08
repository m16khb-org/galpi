"""First-run runtime and model preparation."""

import gc
import json
import os
import shutil
import threading
import time
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from importlib.metadata import version
from pathlib import Path
from typing import cast

from .protocol import EventWriter
from .runtime import (
    configure_warnings,
    detect_torch_device,
    ffmpeg_executable,
    ffmpeg_link_name,
    needs_cpu_fallback,
)

PYANNOTE_MODEL_ID = "pyannote/speaker-diarization-community-1"
QWEN3_ASR_MODEL_ID = "Qwen/Qwen3-ASR-1.7B"
QWEN3_ALIGNER_MODEL_ID = "Qwen/Qwen3-ForcedAligner-0.6B"
QWEN3_MLX_MODEL_DIR_NAME = "qwen3-asr-1.7b-8bit"
QWEN3_MLX_QUANT_BITS = 8
QWEN3_MLX_QUANT_GROUP_SIZE = 64
# Tokenizer/config files the converted MLX model directory must carry so
# Session(model=<dir>) loads without touching the network.
QWEN3_MLX_SIDECAR_FILES = (
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "vocab.json",
    "merges.txt",
    "special_tokens_map.json",
)
# Two model repositories download at once. More would compete for the same
# link without arriving sooner, and each one holds its files in memory while
# it writes them.
DOWNLOAD_WORKERS = 2
DOWNLOAD_PERCENT_START = 10.0
DOWNLOAD_PERCENT_END = 40.0
# Bytes arrive far faster than a person can read; one update a second is
# enough to show movement without flooding the event stream.
DOWNLOAD_REPORT_INTERVAL_SECONDS = 1.0


class DownloadReporter:
    """Turn Hugging Face's progress bars into Galpi phase events.

    `snapshot_download` writes many files at once and reports each through its
    own bar. Summing them gives one honest number for a multi-gigabyte fetch,
    which is what the setup screen needs: without it the bar sits at its
    starting value for the entire download and the app looks stalled.
    """

    def __init__(self, events: EventWriter, start: float, end: float) -> None:
        self._events = events
        self._start = start
        self._end = end
        self._lock = threading.Lock()
        self._downloaded = 0
        self._total = 0
        self._reported_at = 0.0

    def tqdm_class(self) -> type[object]:
        from tqdm.auto import tqdm as base_tqdm

        reporter = self

        class ReportingTqdm(base_tqdm):  # pyright: ignore[reportMissingTypeArgument]
            def __init__(self, *args: object, **kwargs: object) -> None:
                super().__init__(*args, **kwargs)  # pyright: ignore[reportUnknownMemberType]
                reporter.add_total(int(getattr(self, "total", 0) or 0))

            def update(self, n: float | None = 1) -> bool | None:
                reporter.add_progress(int(n or 0))
                return super().update(n)  # pyright: ignore[reportUnknownMemberType, reportUnknownVariableType]

        return ReportingTqdm

    def add_total(self, total: int) -> None:
        with self._lock:
            self._total += total

    def add_progress(self, amount: int) -> None:
        with self._lock:
            self._downloaded += amount
            now = time.monotonic()
            if now - self._reported_at < DOWNLOAD_REPORT_INTERVAL_SECONDS:
                return
            self._reported_at = now
            downloaded, total = self._downloaded, self._total
        if total <= 0:
            return
        share = min(1.0, downloaded / total)
        self._events.emit(
            "phase",
            phase="models",
            percent=round(self._start + (self._end - self._start) * share, 1),
            message=(
                f"모델을 내려받는 중입니다. "
                f"{downloaded / 1_000_000_000:.1f}/{total / 1_000_000_000:.1f} GB"
            ),
        )


def prepare_models(
    manifest: Path,
    engine_bin: Path,
    events: EventWriter,
    engine: str = "whisperx",
) -> None:
    """Install the bundled ffmpeg link and load every model once."""

    configure_warnings()
    link_ffmpeg(engine_bin)
    if engine == "qwen3":
        prepare_qwen3_models(manifest, events)
    else:
        prepare_whisperx_models(manifest, events)


def link_ffmpeg(engine_bin: Path) -> None:
    engine_bin.mkdir(parents=True, exist_ok=True)
    ffmpeg_target = Path(ffmpeg_executable())
    ffmpeg_link = engine_bin / ffmpeg_link_name()
    if ffmpeg_link.exists() or ffmpeg_link.is_symlink():
        ffmpeg_link.unlink()
    try:
        ffmpeg_link.symlink_to(ffmpeg_target)
    except OSError:
        shutil.copy2(ffmpeg_target, ffmpeg_link)
        ffmpeg_link.chmod(0o755)


def prepare_whisperx_models(manifest: Path, events: EventWriter) -> None:
    import whisperx
    from whisperx.diarize import DiarizationPipeline

    device = detect_torch_device()

    events.emit(
        "phase", phase="models", percent=10.0, message="전사 모델을 준비합니다."
    )
    model = whisperx.load_model(
        "large-v3-turbo",
        "cpu",
        compute_type="int8",
        language="ko",
        asr_options={
            "no_speech_threshold": 0.75,
            "condition_on_previous_text": False,
        },
        vad_options={"vad_onset": 0.6, "vad_offset": 0.4},
    )
    del model
    gc.collect()

    events.emit(
        "phase",
        phase="models",
        percent=55.0,
        message=f"{device.upper()}용 한국어 정렬 모델을 준비합니다.",
    )
    alignment_device = device
    try:
        align_model, _metadata = whisperx.load_align_model(
            language_code="ko", device=alignment_device
        )
    except Exception:
        if not needs_cpu_fallback(alignment_device):
            raise
        events.log(
            f"{alignment_device.upper()} 정렬 모델 준비에 실패해 CPU로 다시 시도합니다."
        )
        alignment_device = "cpu"
        align_model, _metadata = whisperx.load_align_model(
            language_code="ko", device=alignment_device
        )
    del align_model
    gc.collect()

    events.emit(
        "phase",
        phase="models",
        percent=78.0,
        message=f"{device.upper()}용 화자분리 모델을 준비합니다.",
    )
    diarization_device = device
    try:
        diarization = DiarizationPipeline(
            model_name="pyannote/speaker-diarization-community-1",
            device=diarization_device,
        )
    except Exception:
        if not needs_cpu_fallback(diarization_device):
            raise
        events.log(
            f"{diarization_device.upper()} 화자분리 모델 준비에 실패해 CPU로 다시 시도합니다."
        )
        diarization_device = "cpu"
        diarization = DiarizationPipeline(
            model_name="pyannote/speaker-diarization-community-1",
            device=diarization_device,
        )
    del diarization
    gc.collect()

    manifest.parent.mkdir(parents=True, exist_ok=True)
    payload: dict[str, object] = {
        "protocol": 1,
        "whisperx": version("whisperx"),
        "torch": version("torch"),
        "pyannote_audio": version("pyannote.audio"),
        "asr_model": "large-v3-turbo",
        "diarization_model": "pyannote/speaker-diarization-community-1",
        "device": diarization_device,
        "alignment_device": alignment_device,
        "asr_device": "cpu",
    }
    write_manifest(manifest, payload)
    events.emit(
        "phase", phase="models", percent=100.0, message="모델 준비가 완료되었습니다."
    )
    events.emit("prepared", engine_version=version("whisperx"))


def prepare_qwen3_models(manifest: Path, events: EventWriter) -> None:
    """Download the Qwen3 stack and convert MLX weights into the cache."""

    import torch
    from huggingface_hub import snapshot_download

    device = detect_torch_device()
    token = os.environ.get("HF_TOKEN") or None

    events.emit(
        "phase",
        phase="models",
        percent=10.0,
        message="Qwen3 전사·정렬 모델을 내려받습니다.",
    )
    # The ASR model, the aligner, and the diarizer are independent downloads of
    # several gigabytes; fetching them one after another leaves the network
    # idle between files and the progress bar parked at one number.
    asr_snapshot: Path | None = None
    with ThreadPoolExecutor(max_workers=DOWNLOAD_WORKERS) as pool:
        reporter = DownloadReporter(
            events, DOWNLOAD_PERCENT_START, DOWNLOAD_PERCENT_END
        )
        snapshots = {
            model_id: pool.submit(
                snapshot_download,
                model_id,
                token=token,
                tqdm_class=reporter.tqdm_class(),
            )
            for model_id in (QWEN3_ASR_MODEL_ID, QWEN3_ALIGNER_MODEL_ID)
        }
        for completed, (model_id, future) in enumerate(snapshots.items(), start=1):
            result = future.result()
            if model_id == QWEN3_ASR_MODEL_ID:
                asr_snapshot = Path(result)
            events.emit(
                "phase",
                phase="models",
                percent=DOWNLOAD_PERCENT_START
                + (DOWNLOAD_PERCENT_END - DOWNLOAD_PERCENT_START)
                * completed
                / len(snapshots),
                message=f"모델을 내려받았습니다. {completed}/{len(snapshots)}",
            )

    events.emit(
        "phase",
        phase="models",
        percent=45.0,
        message="MLX 8비트 가중치로 변환합니다.",
    )
    if asr_snapshot is None:
        raise RuntimeError("Qwen3 ASR 스냅샷을 내려받지 못했습니다.")
    convert_mlx_asr_weights(asr_snapshot, events)

    events.emit(
        "phase",
        phase="models",
        percent=70.0,
        message="화자분리 모델을 내려받습니다.",
    )
    # pyannote's community model is gated: the token from settings travels
    # through the prepare environment, and the diarization pipeline is warmed
    # once so the first real transcription does not pay the download cost.
    from pyannote.audio import Pipeline

    pipeline = Pipeline.from_pretrained(PYANNOTE_MODEL_ID, token=token)
    if device != "cpu":
        pipeline.to(torch.device(device))
    del pipeline
    gc.collect()

    events.emit(
        "phase",
        phase="models",
        percent=90.0,
        message="전사 엔진을 한 번 실행해 확인합니다.",
    )
    verify_qwen3_session(events)

    payload: dict[str, object] = {
        "protocol": 1,
        "qwen3": "2",
        "mlx_qwen3_asr": version("mlx-qwen3-asr"),
        "mlx": version("mlx"),
        "torch": version("torch"),
        "pyannote_audio": version("pyannote.audio"),
        "asr_model": QWEN3_ASR_MODEL_ID,
        "alignment_model": QWEN3_ALIGNER_MODEL_ID,
        "diarization_model": PYANNOTE_MODEL_ID,
        "mlx_quantization": f"{QWEN3_MLX_QUANT_BITS}bit-g{QWEN3_MLX_QUANT_GROUP_SIZE}",
        "device": device,
    }
    write_manifest(manifest, payload)
    events.emit(
        "phase", phase="models", percent=100.0, message="모델 준비가 완료되었습니다."
    )
    events.emit("prepared", engine_version=QWEN3_ASR_MODEL_ID)


def verify_qwen3_session(events: EventWriter) -> None:
    """Load the converted weights once and transcribe a second of silence.

    Preparation used to end at "the files are on disk", so a bad conversion or
    a missing runtime dependency only surfaced when the user started a real
    meeting. Running the engine here moves that failure into the step whose
    job is to report it.
    """

    import numpy as np
    from mlx_qwen3_asr import Session

    from .qwen3 import mlx_asr_model_dir

    asr_dir = mlx_asr_model_dir()
    # Without this the runtime reads the missing directory as a Hugging Face
    # repository id and fails with a validation error about repo names, which
    # tells the user nothing about what actually went wrong.
    if not asr_dir.joinpath("weights.safetensors").is_file():
        raise RuntimeError(f"MLX 가중치 변환 결과를 찾지 못했습니다: {asr_dir}")
    session = Session(model=str(asr_dir))
    try:
        session.transcribe(
            np.zeros(16_000, dtype=np.float32),
            language="Korean",
            return_timestamps=True,
        )
    except Exception as error:
        raise RuntimeError(f"Qwen3 엔진 확인에 실패했습니다: {error}") from error
    finally:
        del session
        gc.collect()
    events.log("Qwen3 엔진 확인을 마쳤습니다.")


def convert_mlx_asr_weights(asr_snapshot: Path, events: EventWriter) -> None:
    """Convert the HF ASR snapshot into quantized MLX weights (one-time).

    The transcription path loads ``cache/mlx/qwen3-asr-1.7b-8bit`` through
    ``Session(model=<dir>)``; conversion builds a sibling temporary
    directory and moves it into place so a crash never leaves a half-built
    model the readiness gate would mistake for a complete one.
    """

    hf_home = os.environ.get("HF_HOME")
    cache_root = Path(hf_home).parent if hf_home else Path.home() / ".cache"
    destination = cache_root / "mlx" / QWEN3_MLX_MODEL_DIR_NAME
    if destination.joinpath("weights.safetensors").is_file():
        return

    import mlx.core as mx
    import mlx.utils as mlx_utils
    from mlx_qwen3_asr.convert import quantize_model
    from mlx_qwen3_asr.load_models import load_model

    # mlx.utils.tree_flatten is annotated with bare Callables upstream and
    # mx.save_safetensors with a bare file alias, which strict mode reads as
    # partially unknown; the conversion shapes are fixed.
    tree_flatten = cast(
        "Callable[[object], list[tuple[str, object]]]", mlx_utils.tree_flatten
    )
    save_safetensors = cast(
        "Callable[[str, dict[str, mx.array]], None]", mx.save_safetensors
    )

    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = destination.with_name(destination.name + ".partial")
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)

    # The public loader resolves the local snapshot, remaps the weights, and
    # casts to float16 in one step; quantization then rewrites the linear
    # layers in place.
    model, _config = load_model(str(asr_snapshot))
    quantize_model(
        model,
        bits=QWEN3_MLX_QUANT_BITS,
        group_size=QWEN3_MLX_QUANT_GROUP_SIZE,
    )
    flat_weights = dict(tree_flatten(cast("object", model.parameters())))
    save_safetensors(
        str(staging / "weights.safetensors"),
        cast("dict[str, mx.array]", flat_weights),
    )
    for name in QWEN3_MLX_SIDECAR_FILES:
        source = asr_snapshot / name
        if source.is_file():
            shutil.copy2(source, staging / name)
    (staging / "quantization_config.json").write_text(
        json.dumps(
            {
                "bits": QWEN3_MLX_QUANT_BITS,
                "group_size": QWEN3_MLX_QUANT_GROUP_SIZE,
            }
        ),
        encoding="utf-8",
    )
    events.log(f"MLX 8비트 가중치 변환 완료: {destination}")
    os.replace(staging, destination)


def write_manifest(manifest: Path, payload: dict[str, object]) -> None:
    manifest.parent.mkdir(parents=True, exist_ok=True)
    temporary = manifest.with_suffix(".tmp")
    temporary.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    temporary.replace(manifest)
