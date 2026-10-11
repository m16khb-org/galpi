"""WhisperX transcription pipeline owned by Galpi."""

from __future__ import annotations

import gc
import json
from pathlib import Path
from typing import TYPE_CHECKING, cast

from .alignment import align_words
from .artifacts import (
    Transcription,
    filter_segments,
    write_json_atomic,
    write_outputs_atomic,
)
from .core import (
    InvalidInput,
    SpeakerHint,
    build_asr_hotwords,
    parse_asr_context,
    validate_speaker_hint,
)
from .protocol import EventWriter
from .runtime import (
    configure_warnings,
    detect_asr_runtime,
    detect_torch_device,
    failure_reason,
    needs_cpu_fallback,
    release_torch_cache,
)
from .speakers import SpeakerTurn, label_segments
from .voice import relabel_lines

if TYPE_CHECKING:
    from collections.abc import Callable

    from whisperx.diarize import DiarizationPipeline, DiarizationSegments


WHISPERX_ENGINE_TAG = "whisperx"
# Diarization hands over at 80% to the per-line voice check, so its own
# library progress must stop below that instead of running up to 99.
DIARIZE_CEILING = 79.0


def checkpoint_reusable(checkpoint_path: Path) -> bool:
    """Accept only a checkpoint this engine could have written.

    Both engines publish `<name>.aligned.v2.json`, so a run that switched
    presets would otherwise read word timings produced by the other stack.
    Files written before the tag existed carry no engine and stay readable.
    """

    if not checkpoint_path.is_file():
        return False
    try:
        payload = json.loads(checkpoint_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    if not isinstance(payload, dict):
        return False
    engine = cast("dict[str, object]", payload).get("engine")
    return engine in (None, WHISPERX_ENGINE_TAG)


def transcribe(
    audio_path: Path,
    output_dir: Path,
    speaker_hint: SpeakerHint,
    events: EventWriter,
    asr_context_path: Path | None = None,
    engine: str = "whisperx",
) -> None:
    """Transcribe with the selected engine preset."""

    if engine == "qwen3":
        from .qwen3 import transcribe_qwen3

        transcribe_qwen3(audio_path, output_dir, speaker_hint, events, asr_context_path)
        return
    transcribe_whisperx(audio_path, output_dir, speaker_hint, events, asr_context_path)


def transcribe_whisperx(
    audio_path: Path,
    output_dir: Path,
    speaker_hint: SpeakerHint,
    events: EventWriter,
    asr_context_path: Path | None = None,
) -> None:
    """Transcribe, align, diarize, filter, and publish artifacts."""

    validate_speaker_hint(speaker_hint)
    if not audio_path.is_file():
        raise InvalidInput(f"audio file not found: {audio_path}")
    output_dir.mkdir(parents=True, exist_ok=True)
    configure_warnings()

    import whisperx
    from whisperx.diarize import DiarizationPipeline

    device = detect_torch_device()
    base_name = audio_path.stem
    checkpoint_path = output_dir / f"{base_name}.aligned.v2.json"
    audio = whisperx.load_audio(str(audio_path))

    if checkpoint_reusable(checkpoint_path):
        events.emit(
            "phase",
            phase="transcribing",
            percent=100.0,
            message="기존 전사 체크포인트를 사용합니다.",
        )
        result = cast(
            Transcription,
            json.loads(checkpoint_path.read_text(encoding="utf-8")),
        )
    else:
        asr_options: dict[str, object] = {
            "no_speech_threshold": 0.75,
            "condition_on_previous_text": False,
        }
        if asr_context_path is not None and asr_context_path.is_file():
            terms, names, aliases = parse_asr_context(
                json.loads(asr_context_path.read_text(encoding="utf-8"))
            )
            hotwords = build_asr_hotwords(terms, names, aliases)
            if hotwords:
                # Bias recognition toward glossary terms and attendee names;
                # the fixed-prompt batched path applies it to every chunk.
                asr_options["hotwords"] = hotwords

        def run_asr(asr_device: str, compute_type: str) -> Transcription:
            message = f"한국어 음성을 전사합니다. (CTranslate2 {asr_device.upper()} {compute_type})"
            events.emit("phase", phase="transcribing", percent=5.0, message=message)
            model = whisperx.load_model(
                "large-v3-turbo",
                asr_device,
                compute_type=compute_type,
                language="ko",
                asr_options=asr_options,
                vad_options={"vad_onset": 0.6, "vad_offset": 0.4},
            )
            return cast(
                Transcription,
                model.transcribe(
                    audio,
                    batch_size=8,
                    language="ko",
                    progress_callback=_phase_progress(
                        events, "transcribing", message, 5.0
                    ),
                ),
            )

        asr_device, compute_type = detect_asr_runtime(device)
        try:
            result = run_asr(asr_device, compute_type)
        except Exception:
            if asr_device == "cpu":
                raise
            events.log(
                f"{asr_device.upper()} 전사에 실패해 CPU int8로 다시 시도합니다."
            )
            gc.collect()
            result = run_asr("cpu", "int8")
        gc.collect()
        events.emit(
            "phase",
            phase="transcribing",
            percent=100.0,
            message="전사가 완료되었습니다.",
        )

        events.emit(
            "phase",
            phase="aligning",
            percent=10.0,
            message=f"{device.upper()}에서 문장과 시간을 정렬합니다.",
        )
        segments = result["segments"]
        failed: str | None = None
        try:
            result = align_words(segments, audio, device)
        except Exception as error:
            if not needs_cpu_fallback(device):
                raise
            failed = failure_reason(error)
        if failed is not None:
            # Retried outside the handler: the traceback holds the failed
            # attempt's tensors until the except block ends.
            events.log(
                f"{device.upper()} 문장 정렬에 실패해 CPU로 다시 시도합니다. ({failed})"
            )
            release_torch_cache()
            result = align_words(segments, audio, "cpu")
        result["engine"] = WHISPERX_ENGINE_TAG
        write_json_atomic(checkpoint_path, result)
        events.emit(
            "phase",
            phase="aligning",
            percent=100.0,
            message="정렬 체크포인트를 저장했습니다.",
        )

    diarize_message = f"{device.upper()}에서 화자를 분리합니다."
    events.emit("phase", phase="diarizing", percent=10.0, message=diarize_message)
    diarization_device = device
    try:
        diarization = DiarizationPipeline(
            model_name="pyannote/speaker-diarization-community-1",
            device=device,
        )
        diarization_segments, centroids = _diarize(
            diarization,
            audio,
            speaker_hint,
            _phase_progress(
                events, "diarizing", diarize_message, 10.0, DIARIZE_CEILING
            ),
        )
    except Exception as error:
        if not needs_cpu_fallback(device):
            raise
        events.log(
            f"{device.upper()} 화자분리에 실패해 CPU로 다시 시도합니다. "
            f"({failure_reason(error)})"
        )
        release_torch_cache()
        diarization_device = "cpu"
        diarization = DiarizationPipeline(
            model_name="pyannote/speaker-diarization-community-1",
            device="cpu",
        )
        diarization_segments, centroids = _diarize(
            diarization,
            audio,
            speaker_hint,
            _phase_progress(
                events, "diarizing", "CPU에서 화자를 분리합니다.", 10.0, DIARIZE_CEILING
            ),
        )
    events.emit(
        "phase",
        phase="diarizing",
        percent=80.0,
        message="문장마다 목소리를 다시 확인합니다.",
    )

    turns = [
        SpeakerTurn(
            start=float(cast(float, row["start"])),
            end=float(cast(float, row["end"])),
            speaker=str(row["speaker"]),
        )
        for row in diarization_segments.to_dict("records")
    ]
    labelled = label_segments(result["segments"], turns)
    duration = len(audio) / 16000
    kept, filtered = filter_segments(labelled, duration)
    # The embedding model the diarization already loaded, so line voices live
    # in the same space as the speaker centroids.
    kept = relabel_lines(
        diarization.model._embedding,  # pyright: ignore[reportPrivateUsage]
        audio,
        kept,
        centroids,
        diarization_device,
        events,
    )
    events.emit(
        "phase", phase="diarizing", percent=100.0, message="화자분리가 완료되었습니다."
    )
    events.emit(
        "phase",
        phase="writing",
        percent=30.0,
        message="환각 구간을 정리하고 결과를 씁니다.",
    )
    srt_path = output_dir / f"{base_name}.srt"
    text_path = output_dir / f"{base_name}_화자별.txt"
    write_outputs_atomic(srt_path, text_path, kept)
    events.emit(
        "phase", phase="writing", percent=100.0, message="결과 파일을 저장했습니다."
    )
    events.emit(
        "completed",
        srt=str(srt_path),
        txt=str(text_path),
        checkpoint=str(checkpoint_path),
        segments=len(kept),
        filtered=len(filtered),
    )


def _phase_progress(
    events: EventWriter,
    phase: str,
    message: str,
    floor: float,
    ceiling: float = 99.0,
) -> Callable[[float], None]:
    """Forward a WhisperX progress callback as phase events.

    ASR and diarization each run as one blocking library call, so without
    this the phase would sit at its starting percent until the call returns.
    100 stays reserved for the completion event of the phase; `ceiling` keeps
    room for a later step inside the same phase.
    """

    def report(percent: float) -> None:
        events.emit(
            "phase",
            phase=phase,
            percent=min(max(percent, floor), ceiling),
            message=message,
        )

    return report


def _diarize(
    pipeline: DiarizationPipeline,
    audio: object,
    hint: SpeakerHint,
    progress: Callable[[float], None],
) -> tuple[DiarizationSegments, dict[str, list[float]]]:
    """Speaker turns plus one centroid embedding per speaker label."""

    if hint.mode == "exact":
        segments, centroids = pipeline(
            audio,
            num_speakers=hint.exact,
            return_embeddings=True,
            progress_callback=progress,
        )
    elif hint.mode == "range":
        segments, centroids = pipeline(
            audio,
            min_speakers=hint.minimum,
            max_speakers=hint.maximum,
            return_embeddings=True,
            progress_callback=progress,
        )
    else:
        segments, centroids = pipeline(
            audio, return_embeddings=True, progress_callback=progress
        )
    return segments, centroids or {}
