"""Transcript refinement orchestration for short and long meetings."""

import json
import os
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

from .assistant_stream import (
    REFINE_STREAM_START_PERCENT,
    request_minutes,
)
from .core import InvalidInput
from .minutes_pipeline import (
    MAP_MAX_WORKERS,
    MAP_PROGRESS_CEILING,
    MAP_PROGRESS_START,
    REDUCE_PROGRESS_CEILING,
    REDUCE_PROGRESS_START,
    MinutesContext,
    TranscriptChunk,
    build_map_messages,
    build_reduce_messages,
    map_progress_band,
    refinement_strategy,
    split_transcript,
)
from .minutes_prompt import (
    NO_BACKGROUND,
    SYSTEM_PROMPT,
    build_messages,
    parse_glossary,
    parse_participants,
    render_glossary,
    render_participants,
    transcript_date,
)
from .protocol import EventWriter

API_KEY_VARIABLE = "GALPI_ASSISTANT_API_KEY"


def write_text_atomic(path: Path, document: str) -> None:
    temporary = path.with_suffix(f"{path.suffix}.tmp")
    temporary.write_text(f"{document}\n", encoding="utf-8")
    temporary.replace(path)


def extract_chunk_notes(
    chunks: list[TranscriptChunk],
    context: MinutesContext,
    model: str,
    api_key: str,
    events: EventWriter,
) -> list[str]:
    """Extract facts from every chunk, several requests at a time.

    The chunks are independent by construction, so they go out concurrently and
    the notes come back in transcript order. Progress reports completed chunks
    instead of streamed characters: with several requests in flight a character
    count would jump backwards as one chunk's stream overtook another's.
    """

    if len(chunks) == 1:
        chunk = chunks[0]
        start, ceiling = map_progress_band(chunk)
        activity = "긴 회의 핵심 사실 추출 (1/1)"
        events.emit(
            "phase",
            phase="refining",
            percent=round(start, 1),
            message=f"{model} 모델로 {activity} 중입니다.",
        )
        return [
            request_minutes(
                build_map_messages(chunk, context),
                model,
                api_key,
                events,
                max(1000, int(len(chunk.text) * 0.25)),
                progress_start=start,
                progress_ceiling=ceiling,
                activity=activity,
            )
        ]

    span = MAP_PROGRESS_CEILING - MAP_PROGRESS_START
    completed = 0
    events.emit(
        "phase",
        phase="refining",
        percent=MAP_PROGRESS_START,
        message=f"{model} 모델로 긴 회의 핵심 사실 추출 중입니다. 0/{len(chunks)} 구간",
    )

    failed = threading.Event()

    def extract(chunk: TranscriptChunk, quiet: EventWriter) -> str | None:
        # Each worker streams into a quiet writer; the caller owns the reporting.
        # After the first failure no new request goes out; None marks a skip.
        if failed.is_set():
            return None
        try:
            return request_minutes(
                build_map_messages(chunk, context),
                model,
                api_key,
                quiet,
                max(1000, int(len(chunk.text) * 0.25)),
            )
        except BaseException:
            failed.set()
            raise

    notes_by_number: dict[int, str] = {}
    with (
        open(os.devnull, "w", encoding="utf-8") as sink,
        ThreadPoolExecutor(max_workers=MAP_MAX_WORKERS) as pool,
    ):
        quiet = EventWriter(stream=sink)
        futures = {pool.submit(extract, chunk, quiet): chunk for chunk in chunks}
        for future in as_completed(futures):
            try:
                note = future.result()
            except BaseException:
                pool.shutdown(wait=True, cancel_futures=True)
                raise
            if note is None or failed.is_set():
                continue
            notes_by_number[futures[future].number] = note
            completed += 1
            events.emit(
                "phase",
                phase="refining",
                percent=round(MAP_PROGRESS_START + span * completed / len(chunks), 1),
                message=(
                    f"{model} 모델로 긴 회의 핵심 사실 추출 중입니다. "
                    f"{completed}/{len(chunks)} 구간"
                ),
            )
    return [notes_by_number[chunk.number] for chunk in chunks]


def refine(
    transcript_path: Path,
    output_path: Path,
    background_path: Path | None,
    participants_path: Path | None,
    glossary_path: Path | None,
    model: str,
    events: EventWriter,
    meeting_date: str | None = None,
) -> None:
    """Refine a speaker-labeled transcript into Korean meeting minutes.

    `meeting_date` is the day the meeting was recorded, which the host reads
    from the audio file. Without it the transcript's own timestamp stands in,
    and that is the day the transcript was written, not the day people met.
    """

    api_key = os.environ.get(API_KEY_VARIABLE, "").strip()
    if not api_key:
        raise InvalidInput("assistant API key is not configured")
    events.emit(
        "phase", phase="refining", percent=10.0, message="전사본을 읽는 중입니다."
    )
    transcript = transcript_path.read_text(encoding="utf-8")
    background = (
        background_path.read_text(encoding="utf-8")
        if background_path is not None and background_path.is_file()
        else ""
    )
    participants = (
        parse_participants(json.loads(participants_path.read_text(encoding="utf-8")))
        if participants_path is not None and participants_path.is_file()
        else []
    )
    glossary = (
        parse_glossary(json.loads(glossary_path.read_text(encoding="utf-8")))
        if glossary_path is not None and glossary_path.is_file()
        else []
    )
    meeting_date = meeting_date or transcript_date(transcript_path)
    context = MinutesContext(
        background=background.strip() or NO_BACKGROUND,
        participants=render_participants(participants),
        glossary=render_glossary(glossary),
        meeting_date=meeting_date,
    )
    if refinement_strategy(transcript) == "single":
        events.emit(
            "phase",
            phase="refining",
            percent=REFINE_STREAM_START_PERCENT,
            message=f"{model} 모델로 회의록을 작성하는 중입니다.",
        )
        expected_chars = max(2000, int(len(transcript) * 0.6))
        document = request_minutes(
            build_messages(
                transcript,
                background,
                participants,
                glossary,
                meeting_date,
            ),
            model,
            api_key,
            events,
            expected_chars,
        )
    else:
        chunks = split_transcript(transcript)
        notes = extract_chunk_notes(chunks, context, model, api_key, events)
        events.emit(
            "phase",
            phase="refining",
            percent=REDUCE_PROGRESS_START,
            message=f"{model} 모델로 최종 회의록을 종합하는 중입니다.",
        )
        document = request_minutes(
            build_reduce_messages(notes, context, SYSTEM_PROMPT),
            model,
            api_key,
            events,
            max(2000, int(sum(len(note) for note in notes) * 0.8)),
            progress_start=REDUCE_PROGRESS_START,
            progress_ceiling=REDUCE_PROGRESS_CEILING,
            activity="최종 회의록 종합",
        )
    events.emit(
        "phase", phase="writing", percent=90.0, message="회의록을 저장하는 중입니다."
    )
    write_text_atomic(output_path, document)
    events.emit("refined", minutes=str(output_path))
