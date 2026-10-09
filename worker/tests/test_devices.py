import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from ..galpi_worker.preparation import link_ffmpeg
from ..galpi_worker.runtime import (
    CPU_ASR,
    TorchDevice,
    ffmpeg_link_name,
    needs_cpu_fallback,
    select_asr_runtime,
    select_torch_device,
)


class DeviceSelectionTests(unittest.TestCase):
    def test_cuda_wins_over_mps_and_cpu(self) -> None:
        # Given/When/Then: priority is cuda > mps > cpu
        self.assertEqual(
            select_torch_device(mps_available=True, cuda_available=True), "cuda"
        )
        self.assertEqual(select_torch_device(mps_available=True), "mps")
        self.assertEqual(select_torch_device(mps_available=False), "cpu")

    def test_fallback_applies_to_any_accelerator(self) -> None:
        self.assertTrue(needs_cpu_fallback("cuda"))
        self.assertTrue(needs_cpu_fallback("mps"))
        self.assertFalse(needs_cpu_fallback("cpu"))

    def test_asr_prefers_mlx_then_cuda_then_cpu_int8(self) -> None:
        # Given/When/Then: MLX wherever mlx-whisper is installed (the macOS
        # lock); CTranslate2 CUDA only when torch and CTranslate2 both see a
        # GPU; CPU int8 otherwise, including Apple MPS without MLX
        mlx = select_asr_runtime("mps", mlx_available=True, ct2_cuda_devices=0)
        self.assertEqual((mlx.backend, mlx.compute_type), ("mlx", "float16"))
        cuda = select_asr_runtime("cuda", mlx_available=False, ct2_cuda_devices=1)
        self.assertEqual(
            (cuda.backend, cuda.device, cuda.compute_type),
            ("ctranslate2", "cuda", "float16"),
        )
        cases: tuple[tuple[TorchDevice, int], ...] = (
            ("cuda", 0),
            ("mps", 1),
            ("cpu", 1),
        )
        for torch_device, gpus in cases:
            self.assertEqual(
                select_asr_runtime(
                    torch_device, mlx_available=False, ct2_cuda_devices=gpus
                ),
                CPU_ASR,
            )

    def test_ffmpeg_link_name_per_platform(self) -> None:
        self.assertEqual(ffmpeg_link_name(platform="win32"), "ffmpeg.exe")
        self.assertEqual(ffmpeg_link_name(platform="darwin"), "ffmpeg")

    def test_link_ffmpeg_creates_link_in_engine_bin(self) -> None:
        from unittest import mock

        with TemporaryDirectory() as raw:
            root = Path(raw)
            target = root / "real-ffmpeg"
            target.write_bytes(b"x")
            with mock.patch(
                "worker.galpi_worker.preparation.ffmpeg_executable",
                return_value=str(target),
            ):
                link_ffmpeg(root / "bin")
            self.assertTrue((root / "bin" / ffmpeg_link_name()).exists())


if __name__ == "__main__":
    unittest.main()
