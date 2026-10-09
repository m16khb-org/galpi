"""Local types for the partially untyped huggingface_hub dependency."""

from os import PathLike

def snapshot_download(
    repo_id: str,
    *,
    repo_type: str | None = ...,
    revision: str | None = ...,
    cache_dir: str | PathLike[str] | None = ...,
    token: bool | str | None = ...,
    force_download: bool = ...,
    local_files_only: bool = ...,
    allow_patterns: list[str] | str | None = ...,
    ignore_patterns: list[str] | str | None = ...,
    tqdm_class: type[object] | None = ...,
) -> str: ...
